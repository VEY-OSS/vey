# Cluster Rate and Speed Limits (Peer Gossip)

Status: draft  
See also: [Redis + control daemon design](cluster_limit.md)

Same scope as the Redis draft: cluster-wide user `request_rate_limit` /
`connection_rate_limit` and `tcp_all_*` / `udp_all_*` speed limits. Local
process limits still exist and still stack. This variant uses **only
`vey-proxy` peers**. No Redis, no `vey-cluster-limit`, no other coordinator.

## Why this variant

The Redis design needs a shared store and a writer daemon. Some deployments
already ship the same user YAML to every proxy and do not want another
runtime dependency. Peers can enforce an approximate cluster cap by
exchanging demand and holding a local slice of the quota.

The trade-off is consistency. Redis Lua is a single atomic bucket. Gossip
is eventual: during one interval, after a crash, or under a partition, the
cluster can overshoot. Bounds are stated below.

## Goals

- No process other than `vey-proxy`. No shared database.
- IO hot path stays on local atomics (`GlobalStreamLimit` / a request token
  bucket). Gossip is a background UDP loop.
- Local YAML limits and cluster YAML limits both apply (AND).
- Membership is explicit (static peers or DNS), not an external service.

## Non-goals

- Strong cluster-wide atomic GCRA
- `request_alive_max`, server/host limits, sock-level limits
- Automatic multi-region / WAN gossip (LAN or well-connected DC only)
- Encrypting the gossip channel (optional HMAC is enough on a private net)

## Roles

```
identical user YAML (local + cluster quotas)
        |
        v
vey-proxy A  <--- UDP gossip --->  vey-proxy B
        ^                               ^
        +----------- vey-proxy C -------+
```

| Component | Owns |
| --- | --- |
| User YAML (rolled out to every proxy) | Process-local quotas **and** cluster quotas |
| Proxy `cluster_limit` | `cluster_id`, listen addr, peer list / DNS, HMAC, interval, fail mode |
| Peer gossip | Membership liveness + per-user demand / TAT / held tokens |

There is no central quota store. If two proxies have different `cluster:`
numbers for Alice, each shapes to its own view and the cluster cap is
undefined. Operators must ship the same user config, which they already do
for passwords and process limits.

## Layering

Unchanged from the Redis draft:

```
new request
  -> local connection / request limiter
  -> cluster connection / request limiter   (local tokens, share from gossip)
  -> IO
       sock speed
       -> process User bucket
       -> cluster UserCluster bucket        (local tokens, share from gossip)
```

`GlobalLimitGroup::UserCluster` is the same stacking point.

## Config

Process:

```yaml
cluster_limit:
  cluster_id: edge-prod-1
  fail_mode: hold_share    # hold_share | absorb | closed
  gossip:
    listen: 0.0.0.0:1921
    advertise: 10.0.0.3:1921
    peers:
      - 10.0.0.1:1921
      - 10.0.0.2:1921
      - 10.0.0.3:1921
    # optional instead of / in addition to static peers:
    # peer_dns: vey-proxy.internal
    interval: 100ms
    suspect_after: 300ms
    dead_after: 1s
    hmac_key: "..."
```

`peers` should list **all** members including self (or `expected_members: 3`
must be set). `expected_members` is what keeps a partition from taking the
full cluster quota; see [Partitions](#partitions).

User (same file on every proxy):

```yaml
- name: alice
  request_rate_limit: 2000/s          # this process
  tcp_all_download_speed_limit: 40M
  cluster:
    request_rate_limit: 8000/s        # all processes together
    connection_rate_limit: 4000/s
    tcp_all_download_speed_limit: 400M
    tcp_all_upload_speed_limit: 200M
    udp_all_download_speed_limit: 50M
    udp_all_upload_speed_limit: 50M
```

No `cluster:` block → no cluster layer for that user. Unmanaged / LDAP
users use the template user's `cluster:` if present, or none.

Missing `cluster_limit` on the process → `cluster:` in user YAML is a
config error (otherwise operators think they have a cluster cap).

# Membership

Each proxy runs a small UDP server on `gossip.listen`.

Every `interval` it sends a **digest** to every configured peer (all-to-all).
N is expected to be small (a handful to a few tens of border proxies).
All-to-all is simpler than SWIM and is fine at that size: 10 peers × 100ms
is 100 packets/s/node of control traffic before payloads.

Peer state:

| State | Meaning |
| --- | --- |
| `alive` | Heard from within `suspect_after` |
| `suspect` | Silent; still counted in `expected_members` |
| `dead` | Silent longer than `dead_after` |

Self is always `alive`. A peer that is not in the configured list is
ignored (no join flood).

`alive_members` = self + peers in `alive` or `suspect`.  
`N_alloc` = `max(alive_members, expected_members)` in `hold_share` mode,
so a partition of 1 still divides by the configured cluster size.

# Rate limit

GCRA TAT is not a CRDT unless clocks agree. This design does **not**
merge TAT. Both rate and speed use the same **share + local token bucket**.

Cluster `request_rate_limit: 8000/s` means a global bucket of 8000 request
tokens per second. Each proxy replenishes its local request bucket at
`allocated_r` (see [Share allocation](#share-allocation)).

`check_rate_limit` stays synchronous:

1. Local process GCRA (today).
2. Cluster request/connection bucket `try_consume(1)`. Empty → deny
   (`RateLimited`), same as today. No `.await`, no Redis RTT.

`connection_rate_limit` still skips reused client connections.

Burst: local `max_burst` for the cluster request bucket is

```
min(configured_burst, allocated_r * gossip_interval * 2)
```

so one proxy cannot dump the whole cluster burst while others are idle
for one interval. Unused burst is not transferred; the next interval’s
share is computed from demand.

# Speed limit

Same idea as the Redis lease design — local consume, background replenish —
but the “global bucket” is a **virtual** one: every peer independently
computes the same shares from the last gossip view and replenishes locally
at that rate. There is no `EVALSHA` and no unused-token return to a store.

## Objects

`PeerStreamLimiter` implements `GlobalStreamLimit` with
`group = UserCluster`:

| Field | Role |
| --- | --- |
| `local_tokens` | Hot-path budget |
| `consumed` | Bytes advanced since last gossip (for EWMA) |
| `ewma_bps` | Demand announced to peers |
| `allocated_bps` | Last computed share; replenish uses this |
| `last_replenish` | `DelayUntil` |

UDP: `PeerDatagramLimiter` with a second EWMA for packets.

Replenish runs on the existing `limit-schedule` runtime, every
`replenish_interval` from the user `cluster:` speed config (default 1s)
**or** every `gossip.interval`, whichever is shorter. Amount added:

```
add = allocated_bps * dt
local_tokens = min(local_tokens + add, local_burst)
```

`local_burst = min(cluster_max_burst, allocated_bps * 2 * gossip.interval)`
so a stale high allocation cannot release a huge burst after the user
has left this node.

`check` / `release` match [`GlobalStreamLimiter`](../../lib/vey-io-ext/src/limit/token_bucket/stream.rs).

## Gossip payload (active users only)

A user/direction is **active** if `consumed > 0` or `local_tokens` are
held or `ewma_bps > 0` in the last few intervals. Idle users are omitted.

Per active direction, each digest carries:

```
user_group, user, kind          # kind = req | conn | tcp_up | tcp_down | udp_*
ewma_bps or ewma_rps            # demand
held_tokens                     # unused local tokens (informational)
epoch                           # gossip sequence of the sender
```

Wire: length-prefixed binary (or capnp), HMAC over body with `hmac_key`.
MTU: pack many users per datagram; split if needed. A 1k active users ×
~32 B is ~32 KiB → a few UDP packets per interval, acceptable.

On receive: store `(peer, user, kind) -> ewma` with the sender’s epoch.
Ignore older epochs. Drop peers that fail HMAC.

## Share allocation

After each interval, for each active `(user, kind)` with cluster quota `R`:

```
demand[self]  = self.ewma
demand[p]     = last ewma from p, or 0 if p is dead
              (suspect still uses last ewma)

sum_d = sum(demand[p] for p in alive_or_suspect) + demand[self]

if sum_d == 0:
    allocated = 0          # nobody sending; do not hoard
else:
    fair = R * demand[self] / sum_d

# hold_share: never give a partition more than R / expected_members * local_alive
if fail_mode == hold_share:
    cap = R * local_alive / expected_members
    allocated = min(fair, cap)
else if fail_mode == absorb:
    allocated = fair       # dead peers' share goes to the living
else:  # closed, should not reach here if gossip is up
    allocated = fair
```

`local_alive` is 1 on this node when computing its own cap in a
single-node partition (`cap = R / expected_members`).

Cold start: `ewma = 0` → `allocated = 0` until the first bytes/requests
are seen. Give a **seed**: if `ewma == 0` but a new connection just
attached, announce `ewma = min_lease_bps` (speed) or `1 rps` (rate) so
the node gets a non-zero slice on the next interval. Until that gossip
round returns, allow `min_lease` bytes / 1 request from a local seed
burst so the handshake is not stuck for 100ms.

EWMA:

```
ewma = 0.5 * (consumed / dt) + 0.5 * ewma
```

Same `alpha` on every node; they do not need the same `ewma` value, only
a consistent *rule*. Allocation uses each node’s **announced** ewma, not
a recomputation of others’ consumed counters.

## Why not return unused tokens

In the Redis design, unused leased tokens go back into one HASH. Here
there is no place to return them. Held tokens stay local until consumed
or they age out:

```
every replenish:
  if consumed == 0 for 2 intervals:
      local_tokens = min(local_tokens, local_burst / 2)   # decay hoard
```

A node that just went idle announces `ewma → 0`. Others raise their
`fair` share on the next interval. The idle node’s leftover
`local_tokens` may still be spent if a connection remains; that is
overshoot, bounded by `local_burst`.

## Worked example

Alice cluster `tcp_all_download_speed_limit: 100M`.  
`expected_members: 4`, `interval: 100ms`.  
`fail_mode: hold_share`.

### One busy proxy

Only A has traffic. A announces 100 MiB/s, B/C/D announce 0 or omit Alice.

```
sum_d = 100 MiB/s
fair_A = 100 MiB/s
cap_A  = 100 * 1 / 4 = 25 MiB/s     # hold_share
allocated_A = 25 MiB/s
```

**This is the important difference from Redis.** With `hold_share`, a
single busy proxy cannot take the full 100 MiB/s just because the others
are idle. It is capped at `R / N`. That is the price of not having a
global bucket: idle peers cannot “hand over” tokens they never held.

If the operator wants idle-to-busy transfer, set `fail_mode: absorb`:

```
fair_A = 100 MiB/s
allocated_A = 100 MiB/s
```

Then one node may use the full cluster cap, and a partition of that node
also uses the full cap. Choose:

| Mode | Idle peers | Partition of 1 |
| --- | --- | --- |
| `hold_share` (default) | Busy node limited to `R/N` | Limited to `R/N` |
| `absorb` | Busy node can reach `R` | Isolated node can reach `R` |
| `closed` | — | Stop cluster replenish if `alive < expected_members` |

`closed` is the only mode that does not overshoot under split brain.
Use it when the cap is a hard limit and a degraded (slower) partition is
acceptable.

### Two busy proxies, absorb

A and B each ~50 MiB/s, `absorb`:

```
fair_A = fair_B = 50 MiB/s
```

Same as the Redis lease steady state, without Lua races. Convergence is
one or two gossip intervals after the load appears.

### Two busy proxies, hold_share

```
cap = 100 * 1 / 4 = 25 MiB/s each
allocated = min(50, 25) = 25 MiB/s each
cluster total = 50 MiB/s
```

The cluster is **under** the 100 MiB/s cap because two of four members
are idle and `hold_share` will not donate their slice. That is
intentional and should be documented for operators: `hold_share` is
“`R/N` per live member, demand-weighted inside that cap”, not “up to R
across the cluster”.

A clearer `hold_share` formula if we want demand-weighting **among the
living** while still reserving slices for the configured size:

```
allocated = R * demand[self] / sum_d * (alive_or_suspect / expected_members)
```

With A+B at 50/50, `alive=4` (C/D suspect or idle-but-alive):
`allocated_A = 100 * 0.5 * (4/4) = 50`. Idle-but-alive peers still
count in `alive`, so the living busy nodes **can** share the full R.
Only when C/D are **dead** (or the network splits) does
`alive/expected` drop and the cap shrink.

**Use this formula.** Idle healthy peers do not steal quota; missing
peers do.

Revised one-busy-proxy + 3 idle alive:

```
sum_d = 100, alive = 4, expected = 4
allocated_A = 100 * 1 * 1 = 100 MiB/s
```

Revised partition of A only (`alive = 1`):

```
allocated_A = 100 * 1 * (1/4) = 25 MiB/s
```

That matches Redis-like “one busy node gets R” **and** safe partitions.

### User moves from A to B

A’s ewma decays (0.5 per interval → ~6% after 4 intervals / 400ms).
B seeds, then climbs. For a few hundred milliseconds Alice is below cap
on B while A still holds `local_burst`. Overshoot ≤ A’s `local_burst`
(typically `allocated_bps * 200ms`).

### Stacked with process-local 40M

A’s socket is min(40 MiB/s local, cluster share). A’s announced ewma is
actual consume (≤ 40 MiB/s), so B+C+D share `R - 40M` in `absorb` /
healthy `hold_share`. Same composition story as the Redis draft.

### Overshoot bound

Per interval, each node replenishes `allocated_bps * dt` using a
**stale** view (up to one `interval` old, plus `suspect_after` if a peer
just died).

```
overshoot ≤ sum_over_nodes (allocated_stale - allocated_true) * dt
         ≤ R * (interval + suspect_after)     # typical, one view late
```

Plus leftover `local_tokens` on nodes that went idle:
`≤ N * local_burst`. Keep `local_burst` to ~2 gossip intervals of the
node’s share.

Crash: the dead node’s leftover tokens die with it (no return path).
Others notice at `dead_after` and raise their share. Under-use, not
overshoot, unless `absorb` plus a **partition** (see below).

## Delay behavior

`local_tokens == 0` → `DelayUntil(last_replenish + replenish_interval)`.
No extra RPC. The next timer tick adds `allocated_bps * dt`. If
`allocated_bps == 0` (no share yet), DelayUntil the next gossip
interval; the seed burst covers new connections.

## Partitions

Two groups that cannot gossip both mark the other `dead`.

| Mode | Each side’s allocation |
| --- | --- |
| `hold_share` | `R * demand_frac * (side_alive / expected_members)` — sum over sides ≤ R |
| `absorb` | each side thinks it is the whole cluster — sum up to `2R` (or `kR`) |
| `closed` | cluster replenish stops on any side with `alive < expected_members` |

Default is `hold_share` with required `expected_members`.

## Fail mode vs gossip loss

If **this** node cannot send/receive (socket error), treat as isolated:

- `hold_share`: keep `allocated = last * (1/expected_members)` or
  recompute with `alive = 1`
- `closed`: stop cluster replenish
- `absorb`: **do not** jump to full R on local socket failure; fall back
  to `hold_share` so a broken NIC does not open the cap

## Attachment

Same call sites as the Redis draft: next to
`user.tcp_all_*_speed_limit()`, add `user.cluster_tcp_all_*()`.
Here `Some` means the user YAML has `cluster.tcp_all_*` **and** process
`cluster_limit` is set. Quota is not discovered at runtime.

HTTP `try_consume(origin_header_size)` must hit the cluster limiter too.

## Metrics

- `cluster_limit.peer_alive` / `suspect` / `dead`
- `cluster_limit.gossip_rx` / `tx` / `hmac_fail` / `bad_epoch`
- `cluster_limit.allocated_bps{user,kind}`
- `cluster_limit.local_starve`
- `cluster_limit.partition_hold` (alive < expected)

## Reload

- Peer list / listen: rebuild gossip socket.
- User `cluster:` numbers: `update()` on the limiter, next allocation
  uses the new `R`.
- Process-local limiters: existing `User::new_for_reload`.

# Comparison with the Redis design

| | Redis + daemon | Peer gossip |
| --- | --- | --- |
| Extra process | Redis + `vey-cluster-limit` | None |
| Cluster quota source | Daemon → Redis HASH | User YAML `cluster:` |
| Rate check | Async Redis GCRA | Sync local bucket |
| Speed hot path | Local tokens | Local tokens |
| Idle → busy handover | Yes (lease leftover) | Yes if peers stay `alive` |
| One node can use full R | Yes | Yes while others are alive |
| Split brain | One Redis view | `hold_share` needed |
| Clock sync | Redis `TIME` | Not required |
| Typical overshoot | `N * max_lease` | `R * interval` + local bursts |

# Implementation order

1. Gossip membership + HMAC + `expected_members`.
2. Cluster request/connection token buckets + share allocation.
3. `PeerStreamLimiter` / `PeerDatagramLimiter` + `UserCluster` stack.
4. `hold_share` / `absorb` / `closed`, metrics, UserGuide.
5. Tests: one node, two nodes equal demand, idle handover, partition
   (`hold_share` sum ≤ R), local+cluster stack, HMAC reject.

# Code touch points

- [`vey-proxy/src/config/mod.rs`](../../vey-proxy/src/config/mod.rs) —
  process `cluster_limit.gossip`
- [`vey-proxy/src/config/auth/user/`](../../vey-proxy/src/config/auth/user/) —
  nested `cluster:` quotas
- `vey-proxy/src/cluster_limit/` — UDP gossip, allocation, limiters
- [`vey-proxy/src/auth/user.rs`](../../vey-proxy/src/auth/user.rs)
- [`lib/vey-io-ext/src/limit/mod.rs`](../../lib/vey-io-ext/src/limit/mod.rs) —
  `GlobalLimitGroup::UserCluster`
- serve task sites: attach cluster speed limiters; cluster rate check
  stays sync
