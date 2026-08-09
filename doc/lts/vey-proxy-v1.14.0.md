# vey-proxy v1.14.0 Release Notes

First LTS release of the **vey-proxy** line after the g3proxy → vey-proxy rebrand.

Compared against **g3proxy 1.12.x** (tip `g3proxy-v1.12.4`).

Many stability fixes from the 1.13 series were already backported to
**g3proxy 1.12.4**. Upgrading 1.12.4 → 1.14.0 is mainly about the new
capabilities, auth models, and compatibility breaks below—not those shared
bugfixes alone.

LTS branch: [`lts/vey-proxy/1.14/default`](https://github.com/VEY-OSS/vey/tree/lts/vey-proxy/1.14/default).

See also [Long-Term Support](../long-term_support.md) and
[vey-proxy/CHANGELOG](../../vey-proxy/CHANGELOG).

## Highlights

### Rebrand and packaging

- Project renamed from **g3proxy** to **vey-proxy** (binary, packages, docs).
- Custom HTTP headers: `X-BD-*` → **`X-VEY-*`**.
- `native_tls_port` → **`usual_tls_port`**.
- Metrics / log / controller app name can follow the **binary name**.
- Optional allocators: jemalloc / mimalloc; package builds prefer
  **secure-snmalloc** where applicable.
- **MSRV 1.94.0**.
- Internal datetime stack: **chrono → jiff** (RFC3339 config strings
  unchanged).

### New user groups

All three reuse the same **static users / dynamic source / cache /
anonymous_user** policy model as `basic`. They differ only in how identity is
proven.

#### `facts` — match by connection facts (no password)

For ports that never see HTTP Basic credentials (transparent / SNI / stream).
Users are selected by `match_by_facts` on connection facts:

| Fact | Match values |
|------|----------------|
| client / server IP | exact `ip:` or CIDR `net:` |
| server name (SNI / Host, etc.) | exact `domain:` or parent `suffix-domain:` (since 1.13.3) |

Enabled on **`tcp_tproxy`**, **`sni_proxy`**, **`tcp_stream`**,
**`tls_stream`**. Rate limits, sites, expire, and other user policy fields work
like a normal user once matched.

```yaml
user_group:
  - name: by_facts
    type: facts
    static_users:
      - name: office
        match_by_facts:
          - net: 10.0.0.0/8
          - suffix-domain: corp.example.net
        tcp_sock_speed_limit: 50M
```

#### `ldap` — remote LDAP simple bind

Password is checked with LDAP/LDAPS **simple bind**. After success, policy
still comes from static/dynamic users, or from an **`unmanaged_user`** template
when the account exists in LDAP but not locally.

- URL: `ldap(s)://host[:port]/base_dn` (`ldap` + `tls_client` → STARTTLS;
  `ldaps` default port 636).
- Username attribute defaults to `uid` (often `cn` on some directories).
- Thread-local password LRU plus connection pool / timeouts for hot paths.

```yaml
user_group:
  - name: corp-ldap
    type: ldap
    ldap_url: ldaps://ldap.example.net/dc=example,dc=net
    username_attribute: uid
    unmanaged_user:
      name: ldap-template
      explicit_sites: []
```

#### `python_basic` — scripted password check

Calls Python `check_password(username, password) -> bool` (needs the `python`
build feature, enabled by default). Policy again comes from static/dynamic or
**`unmanaged_user`**. The script is re-read from disk on each auth attempt
(edit without reloading the group). LRU cache and `check_timeout` limit script
cost.

```yaml
user_group:
  - name: script-auth
    type: python_basic
    script: /etc/vey-proxy/auth_check.py
    unmanaged_user:
      name: python-template
```

```python
def check_password(username, password):
    return username == "admin" and password == "secret"
```

#### Choosing a group

| Type | Proves identity with | Policy from | Best for |
|------|----------------------|-------------|----------|
| `basic` (existing) | Local username/password (or hash) | static / dynamic | Self-managed accounts |
| `facts` | Connection facts (IP / domain) | static / dynamic | Password-less transparent entry |
| `ldap` | LDAP bind | static / dynamic / unmanaged | Corporate directory |
| `python_basic` | Python `check_password` | static / dynamic / unmanaged | Custom credential backends |

Docs: Sphinx `configuration/auth/group/{facts,ldap,python_basic}`.

### Egress context and routing

- Username parameters and HTTP headers can set **egress context**.
- **`comply_context`** escaper selects egress path from that context
  (including egress index).
- Parent upstream address override by egress path on `proxy_http(s)` /
  `proxy_socks5(s)`.
- Nested TCP/UDP connect through route / comply escapers for cleaner chaining.
- `route_upstream` **`suffix_match`** uses **domain hierarchy** (not plain
  string suffix).

### Servers and protocols

- **`udp_stream`** / **`udp_tproxy`** servers.
- **MASQUE CONNECT-UDP** on `http_proxy` and `proxy_http(s)` (draft Well-Known
  URI removed).
- HTTP **`QUERY`** (RFC 10008) for forward / inspect / DPI.
- HTTP/1 and HTTP/2 pass through **1xx informational** responses.
- Upstream **Negotiate** auth; **`proxy-support`** header (including non-HTTP
  proxy escapers).
- Process **`reload`** via RPC/ctl without a full restart.

### Escapers, ICAP, listen, and platform

- **`direct_fixed`**: bind same client IP; **`foreign_port_hint_prefix`** for
  GWLB-style mark/cookie while binding `client_ip:0`.
- ICAP: UNIX socket; **`tcp_connect_timeout`** (default 1s); Host / OPTIONS /
  default-port fixes.
- Linux **eBPF reuseport** (`use_ebpf`) for TCP/UDP and PlainQuicPort
  stickiness; PlainQuicPort Quinn endpoint / payload tuning.
- FreeBSD / OpenBSD transparent listen and foreign bind; FreeBSD
  `user_cookie`, OpenBSD `rtable`.
- NetBSD: `direct_fixed` foreign bind via `IP_BINDANY` (intercept servers still
  limited by NPF).
- Linux **`local_port_range`**; socks_proxy can fall back to it for UDP bind.
- TCP **MSS** in listen config.

### Security

- Prefer local users / root for daemon control.
- Zeroize password material when possible.

### Removals and breaking changes (vs 1.12)

- Drop Lua **5.3** and Python **3.7**.
- Drop custom SOCKS draft `0x09` timeout error code.
- Drop long-deprecated Lua/Python user-source `cache_file`.
- Header / port renames above (`X-VEY-*`, `usual_tls_port`).
- socks_proxy: prefer `udp_relay_yield_count` / `udp_relay_batch_count` over
  deprecated `*_size`.
- User/server option renames from 1.13.0 (`connection_rate_limit`,
  `task_idle_check_interval`, …).

## Upgrade notes (1.12.x → 1.14.0)

1. Replace packages/binaries and config paths for **vey-proxy** naming (see
   [Migration from G3 to VEY](../migrate_from_g3_to_vey.md)).
2. Rename **`X-BD-*` → `X-VEY-*`** and **`native_tls_port` → `usual_tls_port`**.
3. Confirm Lua/Python runtimes (≥ 5.4 / ≥ 3.8) if used.
4. Adopt new auth types only where needed; `basic` remains the default.
5. Review deprecated rate-limit / idle / socks UDP option names.
6. Rebuild requires **Rust 1.94+**.

Detailed per-point history: [vey-proxy/CHANGELOG](../../vey-proxy/CHANGELOG)
(`v1.13.0` … `v1.13.10`).
