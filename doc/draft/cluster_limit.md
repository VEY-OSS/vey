# Cluster Rate and Speed Limits

Status: draft  
See also: [peer gossip design (no Redis)](cluster_limit_peer.md)

Scope: `vey-proxy` user `request_rate_limit` / `connection_rate_limit` and
`tcp_all_*` / `udp_all_*` speed limits across a proxy cluster.

Local process limits and cluster limits are independent layers. They may both
be set. They never replace each other. The effective cap is the stricter of
the layers that are present.

## Goals

- Share one per-user request/connection rate and one per-user bandwidth cap
across all `vey-proxy` processes in the same `cluster_id`.
- Keep the IO hot path off Redis. Byte/packet consumption stays on local
atomics, the same way today's process-wide token bucket works.
- Keep existing user YAML fields as **process-local** quotas. Cluster quotas
live in a central control process, not in each proxy's user config.
- Compose with the existing per-connection sock limits.



## Non-goals (first version)

- `request_alive_max` (concurrency semaphore)
- Server / host / escaper limits
- `log_rate_limit`
- `tcp_sock_speed_limit` / `udp_sock_speed_limit` (already per connection)
- Gossip without Redis (see [cluster_limit_peer.md](cluster_limit_peer.md))



## Current process-local behavior


| Knob                                            | Algorithm                                 | Where it runs                     |
| ----------------------------------------------- | ----------------------------------------- | --------------------------------- |
| `request_rate_limit`, `connection_rate_limit`   | GCRA on `AtomicU64` TAT                   | Once per request / new connection |
| `tcp_all_{upload,download}_speed_limit`         | Token bucket, replenished on a timer      | Every stream read/write           |
| `udp_all_{upload,download}_speed_limit`         | Token bucket (bytes and optional packets) | Every datagram                    |
| `tcp_sock_speed_limit` / `udp_sock_speed_limit` | Per-connection fixed window               | That socket only                  |


Process-wide stream limiting is `[GlobalStreamLimiter](../../lib/vey-io-ext/src/limit/token_bucket/stream.rs)`:
a local `byte_tokens` counter, a background task that adds `replenish_bytes`
every `replenish_interval`, and `check()` that either returns
`AdvanceBy(n)` or `DelayUntil(next_replenish)`.

`[StreamLimiter](../../lib/vey-io-ext/src/limit/stream.rs)` already stacks a
per-connection local limiter plus any number of global limiters, and takes
the minimum `AdvanceBy`. Cluster bandwidth is one more global limiter on
that list.

## Roles

```
operator YAML
      |
      v
vey-cluster-limit (control daemon)
      |  writes quota HASH, deletes removed users
      v
    Redis
      ^
      |  EVALSHA GCRA / lease  (reads quota HASH, updates state HASH)
      |
vey-proxy  x N     + local user YAML (process limits only)
```


| Component               | Owns                                                | Does not own      |
| ----------------------- | --------------------------------------------------- | ----------------- |
| User YAML on each proxy | Process-local rate and `*_all_*` speed limits       | Any cluster quota |
| Proxy `cluster_limit`   | `cluster_id`, Redis client, fail mode, lease timing | Per-user numbers  |
| `vey-cluster-limit`     | Central per-user cluster quotas; writes Redis       | Traffic           |
| Redis                   | Quota definition + GCRA TAT + bandwidth buckets     | —                 |


Missing quota key for a user means “no cluster limit for that user”.
LDAP / unmanaged users are looked up by `user_group` + username the same way.

## Layering

```
new request
  -> local connection GCRA (new TCP/QUIC only)
  -> local request GCRA
  -> Redis connection GCRA (skip if no quota)
  -> Redis request GCRA (skip if no quota)
  -> IO
       sock speed limit
       -> process User token bucket
       -> cluster UserCluster token bucket
       -> advance min(granted)
```

Rate checks are AND: any configured layer that rejects, rejects.

Bandwidth checks are AND via stacked `GlobalStreamLimit` objects. If the
cluster limiter grants 32 KiB and the process limiter grants 8 KiB, the
socket advances 8 KiB and the extra 24 KiB is `release()`d back to the
cluster bucket.

`GlobalLimitGroup` gains `UserCluster`. HTTP pipeline code that calls
`retain_global_limiter_by_group(Server)` runs **before** user limiters are
attached, so cluster limiters must be added in the same place as today's
`User` limiters, not earlier.

## Proxy process config

No per-user cluster fields.

```yaml
cluster_limit:
  cluster_id: edge-prod-1
  fail_mode: open          # open | closed
  redis:
    addr: redis.internal:6379
    db: 2
  rate_limit:
    timeout: 20ms
  speed_limit:
    lease_interval: 200ms
    min_lease: 16KB
    min_retry: 20ms
```

Unconfigured `cluster_limit` keeps today's behavior exactly.

## Control daemon config

```yaml
cluster_id: edge-prod-1
redis:
  addr: redis.internal:6379
  db: 2
user:
  - group: default
    name: alice
    request_rate_limit: 8000/s
    connection_rate_limit: 4000/s
    tcp_all_download_speed_limit: 400M
    tcp_all_upload_speed_limit: 200M
    udp_all_download_speed_limit: 50M
    udp_all_upload_speed_limit: 50M
```

On start and on reload the daemon writes quota hashes and `DEL`s users that
disappeared. A periodic reconcile pass repairs drift. Quota values are stored
as integers so Lua never parses `400M/s`.

# Redis schema

Identity: `{cluster_id}:{user_group}:{user}`.

## Quota (daemon writes, no TTL)

```
vey:climit:quota:{id}:{group}:{user}   HASH
```


| Field                                                                        | Meaning                             |
| ---------------------------------------------------------------------------- | ----------------------------------- |
| `request_rate_replenish_nanos`                                               | GCRA cell interval                  |
| `request_rate_max_burst`                                                     | GCRA burst (cells)                  |
| `connection_rate_replenish_nanos`                                            |                                     |
| `connection_rate_max_burst`                                                  |                                     |
| `tcp_up_replenish_bytes`                                                     | Bytes added each replenish interval |
| `tcp_up_interval_ms`                                                         | Replenish interval                  |
| `tcp_up_max_burst`                                                           | Token cap                           |
| `tcp_down_*`                                                                 | Same for download                   |
| `udp_up_replenish_bytes` / `udp_up_replenish_packets`                        |                                     |
| `udp_up_interval_ms` / `udp_up_max_burst_bytes` / `udp_up_max_burst_packets` |                                     |
| `udp_down_*`                                                                 |                                     |


A missing hash, or a missing field group, means that limit is unset.

`100M` in YAML becomes `tcp_down_replenish_bytes=104857600`,
`tcp_down_interval_ms=1000`, `tcp_down_max_burst=104857600` unless the
operator sets an explicit burst / interval (same shape as
`GlobalStreamSpeedLimitConfig`).

## Runtime state (proxy Lua writes)

```
vey:climit:state:{id}:{group}:{user}:req        STRING  TAT microseconds, PX TTL
vey:climit:state:{id}:{group}:{user}:conn       STRING  TAT microseconds, PX TTL
vey:climit:state:{id}:{group}:{user}:tcp_up     HASH    tokens, last_us
vey:climit:state:{id}:{group}:{user}:tcp_down   HASH    tokens, last_us
vey:climit:state:{id}:{group}:{user}:udp_up     HASH    byte_tokens, packet_tokens, last_us
vey:climit:state:{id}:{group}:{user}:udp_down   HASH    byte_tokens, packet_tokens, last_us
```

Rate-limit TAT keys expire after about two burst windows so idle users do
not leak. Bandwidth state hashes can use a longer TTL (for example 60s)
refreshed on each lease; if every instance stops leasing, the hash goes
away and the next lease starts from a full bucket.

# Rate limit (summary)

Same GCRA as `[RateLimiter](../../lib/vey-types/src/limit/rate/mod.rs)`, but TAT
is Unix-epoch microseconds from Redis `TIME` so instances do not need
synchronized clocks.

Lua: if the quota fields are absent, return allow. Otherwise:

```
now = redis TIME in microseconds
earliest = tat - (max_burst - 1) * replenish_nanos
if now < earliest: return wait
tat = max(tat, now) + replenish_nanos
SET PX
return 0
```

`connection_rate_limit` still applies only when the client connection is not
reused. `request_rate_limit` applies every request, including HTTP keepalive.

`check_rate_limit` becomes async. Timeout uses `fail_mode`. Default `open`
admits the request and increments a metric.

Scripts are `SCRIPT LOAD`ed once and called with `EVALSHA`.

# Speed limit

This is the part that cannot be a Redis round-trip per byte.

## Why not Redis on the hot path

A 1 Gbps flow is ~80k packets/s and far more `read`/`write` calls. Each
`GlobalStreamLimiter::check` is a local `AtomicU64` CAS. Putting GCRA or
`INCR` on Redis here would add tens of microseconds and a hard availability
dependency to every buffer.

The process-local limiter already splits “consume tokens” (hot) from
“replenish tokens” (timer). Cluster limiting keeps that split: replenish
becomes “lease a slice from Redis”, consume stays local.

## Objects

Each configured direction is one bucket:

- `tcp_up`, `tcp_down`
- `udp_up`, `udp_down` (bytes and packets in one hash)

On a proxy, `ClusterStreamLimiter` implements `GlobalStreamLimit` with
`group = UserCluster`:


| Field                     | Role                                     |
| ------------------------- | ---------------------------------------- |
| `local_tokens: AtomicU64` | Hot-path budget                          |
| `consumed: AtomicU64`     | Bytes actually advanced since last lease |
| `last_grant: Instant`     | For `DelayUntil`                         |
| `ewma_bps: AtomicU64`     | Demand for the next request              |
| `lease_in_flight`         | Coalesce extra lease attempts            |


UDP uses a `ClusterDatagramLimiter` with a second atomic for packets.

When `cluster_limit` is enabled, every user gets these limiters attached
the same way process `tcp_all_*` limiters are attached today. If Redis has
no quota for that user, `check()` always returns `AdvanceBy(to_advance)`
and the leaser does not call Redis.

## Local consume (unchanged shape)

Same contract as `GlobalStreamLimiter`:

```
check(to_advance):
  n = try_consume(to_advance)   # CAS subtract, may grant partial
  if n > 0: return AdvanceBy(n)
  else:     return DelayUntil(last_grant + lease_interval)

release(size):
  add local_tokens, capped by max_lease  # unread bytes given back
```

Partial consume matters: a 64 KiB read against 8 KiB remaining tokens
advances 8 KiB, then waits. `StreamLimiter` already shrinks later global
limiters via `release(checked - to_advance)` when an earlier limiter
grants less.

`consumed` counts only bytes that were not `release()`d, i.e. bytes that
really left the limiter.

## Lease cycle

A background task per limiter (same `limit-schedule` runtime as today's
replenish task) runs every `lease_interval` (default 200ms), and also
when `local_tokens` hits 0 if `min_retry` has elapsed.

```
                        Redis bucket                         instance A
                   (one HASH per direction)                 local_tokens
                            |                                     |
  refill by elapsed time    |                                     |
  add unused from instance  |<---- lease(requested, unused) ------|
  grant = min(requested,    |---- granted ----------------------->|
            tokens)         |                               replace local
                            |                               update ewma
```

Sequence for one instance:

1. `unused = local_tokens.swap(0)`
2. `used = consumed.swap(0)`
3. `dt = now - last_lease`
4. `ewma_bps = alpha * (used / dt) + (1 - alpha) * ewma_bps`
  `alpha = 0.5` is enough; it tracks a user moving from one proxy to
   another within a few intervals.
5. `requested = clamp(ewma_bps * lease_interval * 1.5, min_lease, max_lease)`
6. If `used == 0` and `unused == 0` and `ewma_bps == 0` and there is no
  attached connection interest, **skip Redis** (idle).
7. `granted = EVALSHA lease(requested, unused)`
8. `local_tokens.store(granted)`, `last_grant = now`

Idle skip avoids a Redis chat for users that only have a process-local
limit. “Attached connection interest” is `Arc::strong_count` on the
limiter above the leaser itself, which matches how
`GlobalStreamLimiter::tokio_spawn_replenish` already exits.

Cold start: `ewma_bps = 0`, so the first lease asks `min_lease` (16 KiB).
A large download then burns it immediately, triggers `min_retry` extra
leases, and EWMA climbs to `max_lease`.

### `max_lease`

Default:

```
max_lease = replenish_bytes * lease_interval / replenish_interval
```

For `tcp_all_download_speed_limit: 100M` and `lease_interval: 200ms`:

```
max_lease = 100 MiB/s * 0.2 s = 20 MiB
```

This is the most one instance may hold. It is also the main overshoot
bound (see below). Operators can set a smaller `max_lease` in proxy
`cluster_limit.speed_limit` if they want tighter fairness at the cost of
more Redis chatter.

`min_lease` exists so a newly busy instance can start without waiting for
EWMA. It must stay small (tens of KiB).

### Headroom `1.5`

`requested = ewma * interval * 1.5` so a rising flow is not clipped every
interval. Redis still caps `grant` by the global bucket, so headroom
cannot exceed the cluster cap. If two instances both ask 1.5× of half the
rate, they ask 0.75× of the full rate each; the Lua serializes grants and
the second one gets what is left. EWMA then falls on the starved instance
and rises on the one that actually sent, which is the desired fairness.

## Lease Lua

Keys: `KEYS[1] = state HASH`, `KEYS[2] = quota HASH`.  
ARGV: `requested`, `unused_returned`, field prefix (`tcp_down` / …).

Integer microseconds from `TIME`. Integer byte math; refill uses a 128-bit
intermediate in Lua (`tokens + elapsed_us * replenish_bytes / interval_us`).

```
local replenish = tonumber(redis.call('HGET', KEYS[2], prefix .. '_replenish_bytes') or '0')
if replenish == 0 then
  return -1   -- no cluster quota; proxy treats as unlimited
end
local interval_ms = tonumber(redis.call('HGET', KEYS[2], prefix .. '_interval_ms'))
local max_burst   = tonumber(redis.call('HGET', KEYS[2], prefix .. '_max_burst'))
local requested   = tonumber(ARGV[1])
local unused      = tonumber(ARGV[2])

local t = redis.call('TIME')
local now_us = tonumber(t[1]) * 1000000 + tonumber(t[2])

local tokens  = tonumber(redis.call('HGET', KEYS[1], 'tokens') or max_burst)
local last_us = tonumber(redis.call('HGET', KEYS[1], 'last_us') or now_us)

tokens = tokens + unused
if tokens > max_burst then tokens = max_burst end

local elapsed = now_us - last_us
if elapsed < 0 then elapsed = 0 end
local refill = elapsed * replenish / (interval_ms * 1000)
tokens = tokens + refill
if tokens > max_burst then tokens = max_burst end

local granted = requested
if granted > tokens then granted = tokens end
tokens = tokens - granted

redis.call('HSET', KEYS[1], 'tokens', math.floor(tokens), 'last_us', now_us)
redis.call('PEXPIRE', KEYS[1], 60000)
return math.floor(granted)
```

`-1` → proxy disables waiting (`check` always advances).  
`0` → local bucket empty, `DelayUntil(now + min_retry)` so we try again
soon instead of sleeping a full `lease_interval` while Redis is empty
because another instance holds the tokens (they will return `unused` on
their next cycle).

UDP Lua is the same with a second `(packet_tokens, packet_requested, packet_unused)` pair in one script so bytes and packets stay consistent.

Lua is atomic per key. Concurrent leases from N proxies are serialized;
no extra lock.

## Worked example

Cluster quota: Alice `tcp_all_download_speed_limit: 100M` (104,857,600 B/s),
burst = 100 MiB, interval = 1s.  
Four proxies. `lease_interval = 200ms`, `min_lease = 16KiB`,
`max_lease = 20MiB`. Redis refill ≈ 20 MiB every 200ms.

### One busy proxy

Alice downloads only through proxy A.


| Time      | A ewma    | A requests                    | Redis tokens before | Grant A | Redis after |
| --------- | --------- | ----------------------------- | ------------------- | ------- | ----------- |
| 0–200ms   | ~0        | 16 KiB                        | 100 MiB             | 16 KiB  | ~100 MiB    |
| 200–400ms | climbing  | up to 20 MiB                  | ~100 MiB            | 20 MiB  | ~80 MiB     |
| steady    | 100 MiB/s | 20 MiB × 1.5 capped at 20 MiB | ~20 MiB refill      | 20 MiB  | ~0          |


A's local `check()` therefore sees ~20 MiB every 200ms → 100 MiB/s.
B/C/D are idle and skip Redis.

### Two busy proxies, equal demand

A and B each carry ~50 MiB/s.

Each asks `50MiB/s * 0.2s * 1.5 = 15 MiB`. Redis has ~20 MiB after refill.

First EVALSHA grants 15 MiB, second grants 5 MiB. The starved instance's
EWMA drops, the other rises, next round the asks are closer to 10+10.
Oscillation stays inside one `max_lease`. Long-term average is 50/50
within a few intervals.

### User moves from A to B

A still holds up to 20 MiB unused. Next A lease returns it (`unused`).
B starts from `min_lease` and ramps. For at most one interval Alice may
be below cap on B while A returns tokens. That is the fairness delay,
not extra bandwidth.

### Stacked with a process-local cap

Proxy A also has `tcp_all_download_speed_limit: 40M` in user YAML.

IO stacks `User` (40 MiB/s local bucket) and `UserCluster` (leased from
100 MiB/s). The socket cannot exceed 40 MiB/s on A. Cluster Redis still
sees A's consumption via `used`, so B+C+D share the remaining ~60 MiB/s.
This is the point of not overriding local limits: one instance can be
pinned without opening the cluster cap.

### Overshoot bound

Tokens granted to an instance are already subtracted from Redis. Until
the next lease, Redis cannot see unused local tokens.

Worst case: N instances each hold `max_lease` and all consume it after
the global bucket is already empty. Extra bytes relative to a perfect
shaper:

```
overshoot ≤ N * max_lease
         ≤ N * R * lease_interval     (R = configured B/s)
```

With N=4, R=100 MiB/s, 200ms: ≤ 80 MiB ≈ 800ms of quota. Typical case is
closer to `1 * max_lease` because idle instances do not lease.

Crash of an instance: its `unused` is never returned. Redis has already
subtracted `granted`. Time-based refill restores the bucket at rate R;
the lost slice is at most `max_lease`. Acceptable, same order as the
overshoot bound.

Do **not** replenish locally at full `R` while Redis is healthy. That is
today's process-wide bug multiplied by N.

## Delay behavior

When `local_tokens == 0`:

1. If a lease is not in flight and `min_retry` elapsed, spawn an extra
  lease (rate-limited).
2. Return `DelayUntil(last_grant + lease_interval)`, or
  `now + min_retry` if the last Lua grant was 0.

The existing limited reader/writer already sleeps on `DelayUntil`. No IO
path change beyond `add_global_limiter(cluster)`.

## Fail mode

Redis error or timeout on lease:


| `fail_mode`      | Behavior                                                                                                                                                                                                                            |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `open` (default) | Temporarily replenish locally at the **cluster** rate R (same as one process owning the whole quota). Metric `cluster_limit.speed_fail_open`. When Redis returns, go back to lease. N proxies all fail-open can send up to `N * R`. |
| `closed`         | Stop replenishing. Drain `local_tokens`, then `DelayUntil` forever until Redis works. Metric `cluster_limit.speed_fail_closed`.                                                                                                     |


Fail-open is availability-first. Use fail-closed when the cluster cap is
a hard billing/legal limit.

Quota HASH read failures use the last cached quota in the limiter (from
the previous successful lease). If there has never been a quota, treat as
unset (unlimited cluster layer).

## Attachment points

Same places that already do:

```
if let Some(limiter) = user.tcp_all_upload_speed_limit() {
    clt_r.add_global_limiter(limiter.clone());
}
if let Some(limiter) = user.tcp_all_download_speed_limit() {
    clt_w.add_global_limiter(limiter.clone());
}
```

also:

```
if let Some(limiter) = user.cluster_tcp_all_upload_speed_limit() {
    clt_r.add_global_limiter(limiter.clone());
}
if let Some(limiter) = user.cluster_tcp_all_download_speed_limit() {
    clt_w.add_global_limiter(limiter.clone());
}
```

The `cluster_*` accessors return `Some` whenever process `cluster_limit`
is enabled, not when YAML has a number. Quota presence is decided in
Redis so the control daemon can add a cap without proxy reload.

Header bytes consumed via `try_consume(origin_header_size)` today must
call the cluster limiter too, otherwise HTTP request lines bypass the
cluster bucket.

## Metrics

- `cluster_limit.redis_error`, `timeout`
- `cluster_limit.rate_denied` (also existing user `rate_limited` forbid stat)
- `cluster_limit.lease_granted_bytes`, `lease_unused_returned`
- `cluster_limit.local_starve` (check hit zero tokens)
- `cluster_limit.speed_fail_open` / `speed_fail_closed`
- `cluster_limit.quota_miss` (Lua returned -1)



## Reload

- Proxy `cluster_limit` Redis client: rebuild on config reload.
- Process-local user speed/rate: existing `User::new_for_reload` (reuse
limiter objects, `update(config)`).
- Cluster quota change: next lease/GCRA reads the new HASH. No proxy
reload.
- Daemon reload: rewrite hashes, `DEL` removed users. In-flight state
keys stay; a deleted quota makes Lua return `-1` (unlimited) or, if we
prefer fail-closed-on-delete, return `0` forever. First version: `-1`
(quota gone means cluster layer off).



# Implementation order

1. Freeze this Redis schema (the only protocol between daemon and proxy).
2. `vey-cluster-limit`: YAML, write/delete quota, reload, ctl.
3. Proxy: `cluster_limit` + async GCRA after local checks.
4. Proxy: lease bandwidth + `UserCluster` group stacked with `User`.
5. Fail mode, metrics, UserGuide, tests.

Tests that matter for bandwidth:

- One fake instance: leased rate matches quota.
- Two fake instances: sum of `used` stays ≤ quota + `max_lease`.
- Local 40M + cluster 100M on one instance: observed ≤ 40M, sibling
instance can use ~60M.
- Quota HASH deleted: cluster layer becomes unlimited, local still applies.
- Redis down + `fail_closed`: traffic stalls after local tokens drain.



# Code touch points

- New crate `vey-cluster-limit`
- `[vey-proxy/src/config/mod.rs](../../vey-proxy/src/config/mod.rs)` — process
`cluster_limit` only
- `vey-proxy/src/cluster_limit/` — pool, Lua, GCRA, lease
- `[vey-proxy/src/auth/user.rs](../../vey-proxy/src/auth/user.rs)` — stack
local + cluster limiters; async `check_rate_limit`
- `[lib/vey-io-ext/src/limit/mod.rs](../../lib/vey-io-ext/src/limit/mod.rs)` —
`GlobalLimitGroup::UserCluster`
- `vey-proxy/src/serve/*/task.rs` — `.await` rate check; add cluster
speed limiters next to existing user ones
- `[vey-proxy/UserGuide.en_US.md](../../vey-proxy/UserGuide.en_US.md)` —
process vs cluster layers

