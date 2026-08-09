# vey-proxy v1.14.0 发布说明

g3proxy 更名为 **vey-proxy** 之后的首个 LTS 版本。

对比基线：**g3proxy 1.12.x**（最新标签 `g3proxy-v1.12.4`）。

1.13 系列中的大量稳定性修复已回移到 **g3proxy 1.12.4**。从 1.12.4 升级到
1.14.0，重点在于下文的新能力、认证模型与兼容性变更，而非那些双方已共有的
bugfix。

LTS 分支：[`lts/vey-proxy/1.14/default`](https://github.com/VEY-OSS/vey/tree/lts/vey-proxy/1.14/default)。

另见 [Long-Term Support](../long-term_support.md)、
[vey-proxy/CHANGELOG](../../vey-proxy/CHANGELOG)，以及
[英文版发布说明](vey-proxy-v1.14.0.md)。

## 要点

### 品牌与打包

- 工程由 **g3proxy** 更名为 **vey-proxy**（二进制、软件包、文档）。
- 自定义 HTTP 头：`X-BD-*` → **`X-VEY-*`**。
- `native_tls_port` → **`usual_tls_port`**。
- metrics / log / controller 的应用名可跟随 **二进制文件名**。
- 可选全局分配器：jemalloc / mimalloc；软件包构建在适用场景下优先
  **secure-snmalloc**。
- **MSRV 1.94.0**。
- 内部日期时间库：**chrono → jiff**（配置中的 RFC3339 字符串不变）。

### 新增用户组

以下三类与既有 `basic` 共用同一套 **static users / dynamic source / cache /
anonymous_user** 策略模型，差别仅在于**如何证明身份**。

#### `facts` — 按连接事实匹配（无密码）

适用于看不到 HTTP Basic 凭证的入口（透明代理 / SNI / stream）。根据连接上的
auth facts，用用户配置中的 `match_by_facts` 选中用户：

| 事实 | 匹配方式 |
|------|----------|
| 客户端 / 服务端 IP | 精确 `ip:`，或 CIDR `net:` |
| 服务名（SNI / Host 等） | 精确 `domain:`，或父域 `suffix-domain:`（自 1.13.3） |

已在 **`tcp_tproxy`**、**`sni_proxy`**、**`tcp_stream`**、**`tls_stream`** 上启用。
匹配成功后，限速、站点、过期等用户策略字段与普通用户相同。

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

#### `ldap` — 远端 LDAP simple bind

用 LDAP/LDAPS **simple bind** 校验密码。通过后，策略仍来自 static / dynamic
用户列表；若 LDAP 有账号、本地未单独建档，可使用 **`unmanaged_user`** 模板。

- URL：`ldap(s)://host[:port]/base_dn`（`ldap` + `tls_client` → STARTTLS；
  `ldaps` 默认端口 636）。
- 用户名属性默认 `uid`（部分目录常用 `cn`）。
- 线程本地密码 LRU，以及连接池 / 超时，用于热路径。

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

#### `python_basic` — 脚本校验密码

调用 Python `check_password(username, password) -> bool`（需 `python` 构建
特性，默认开启）。策略同样来自 static / dynamic 或 **`unmanaged_user`**。
每次认证会重新读取脚本文件（改脚本通常无需 reload 用户组）。LRU 缓存与
`check_timeout` 用于控制脚本开销。

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

#### 选型对照

| 类型 | 身份证明 | 策略来源 | 适合 |
|------|----------|----------|------|
| `basic`（原有） | 本地用户名/密码（或 hash） | static / dynamic | 自管账号 |
| `facts` | 连接 facts（IP / 域名） | static / dynamic | 无口令透明入口 |
| `ldap` | LDAP bind | static / dynamic / unmanaged | 企业目录 |
| `python_basic` | Python `check_password` | static / dynamic / unmanaged | 定制校验逻辑 |

详细配置见 Sphinx：`configuration/auth/group/{facts,ldap,python_basic}`。

### Egress 上下文与路由

- 可通过用户名参数与 HTTP 头设置 **egress context**。
- **`comply_context`** escaper 根据该上下文选择 egress 路径（含 egress
  index）。
- `proxy_http(s)` / `proxy_socks5(s)` 可按 egress 路径覆盖上游父代理地址。
- 经 route / comply escaper 的嵌套 TCP/UDP connect，链路更清晰。
- `route_upstream` 的 **`suffix_match`** 按 **域名层级** 匹配（不再是纯字符串后缀）。

### 服务器与协议

- **`udp_stream`** / **`udp_tproxy`** 服务器。
- `http_proxy` 与 `proxy_http(s)` 支持 **MASQUE CONNECT-UDP**（已移除 draft
  Well-Known URI）。
- HTTP **`QUERY`**（RFC 10008）用于转发 / 检测 / DPI。
- HTTP/1 与 HTTP/2 向客户端透传 **1xx 信息性响应**。
- 上游 **Negotiate** 认证；**`proxy-support`** 头（含非 HTTP 代理 escaper）。
- 可通过 RPC/ctl **`reload`** 热重载进程，无需完整重启。

### Escaper、ICAP、监听与平台

- **`direct_fixed`**：可绑定与客户端相同 IP；**`foreign_port_hint_prefix`**
  在绑定 `client_ip:0` 时把客户端端口编入 mark/cookie（利于 GWLB，减少
  `EADDRINUSE`）。
- ICAP：UNIX 套接字；**`tcp_connect_timeout`**（默认 1s）；Host / OPTIONS /
  默认端口相关修复。
- Linux **eBPF reuseport**（`use_ebpf`）用于 TCP/UDP 与 PlainQuicPort 粘滞；
  PlainQuicPort 支持 Quinn endpoint / 载荷调优。
- FreeBSD / OpenBSD 透明监听与 foreign bind；FreeBSD `user_cookie`、OpenBSD
  `rtable`。
- NetBSD：`direct_fixed` 可通过 `IP_BINDANY` foreign bind（拦截类 server 仍受
  NPF 限制）。
- Linux **`local_port_range`**；socks_proxy 在未设置 `udp_bind_port_range` 时可回退使用。
- 监听配置支持 TCP **MSS**。

### 安全

- 控制面优先仅允许本机用户与 root。
- 尽可能对密码材料做 zeroize。

### 相对 1.12 的移除与破坏性变更

- 不再支持 Lua **5.3** 与 Python **3.7**。
- 移除自定义 SOCKS draft `0x09` 超时错误码。
- 移除 Lua/Python 用户源中长期废弃的 `cache_file`。
- 上述头与端口重命名（`X-VEY-*`、`usual_tls_port`）。
- socks_proxy：优先使用 `udp_relay_yield_count` / `udp_relay_batch_count`，替代废弃的 `*_size`。
- 1.13.0 起的用户/服务器选项重命名（如 `connection_rate_limit`、
  `task_idle_check_interval` 等）。

## 升级说明（1.12.x → 1.14.0）

1. 按 **vey-proxy** 命名替换软件包/二进制与配置路径（参见
   [从 G3 迁移到 VEY](../migrate_from_g3_to_vey.md)）。
2. 将 **`X-BD-*` → `X-VEY-*`**，**`native_tls_port` → `usual_tls_port`**。
3. 若使用脚本认证，确认 Lua/Python 运行时版本（≥ 5.4 / ≥ 3.8）。
4. 仅在需要时采用新认证类型；默认仍为 `basic`。
5. 检查已废弃的限速 / idle / socks UDP 选项名。
6. 从源码构建需要 **Rust 1.94+**。

逐条变更历史见 [vey-proxy/CHANGELOG](../../vey-proxy/CHANGELOG)
（`v1.13.0` … `v1.13.10`）。
