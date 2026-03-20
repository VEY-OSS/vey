# vey-proxy User Guide

**Table of Contents**

- [Installation](#installation)
- [Basic Concepts](#basic-concepts)
    + [Service Management](#service-management)
    + [Hot Upgrades](#hot-upgrades)
    + [Configuration Structure](#configuration-structure)
    + [Monitoring](#monitoring)
- [Basic Usage](#basic-usage)
    + [HTTP Proxy](#http-proxy)
    + [SOCKS Proxy](#socks-proxy)
    + [TCP Mapping](#tcp-mapping)
    + [TLS Offloading](#tls-offloading)
    + [TLS Encapsulation](#tls-encapsulation)
    + [SNI Proxy](#sni-proxy)
    + [Transparent Proxy](#transparent-proxy)
    + [Route Binding](#route-binding)
    + [Proxy Chaining](#proxy-chaining)
    + [Connection Rate Limits](#connection-rate-limits)
    + [Process-Wide Rate Limits](#process-wide-rate-limits)
    + [DNS Resolution](#dns-resolution)
    + [Secure DNS Resolution](#secure-dns-resolution)
    + [Failover DNS Resolution](#failover-dns-resolution)
    + [User Authentication and Authorization](#user-authentication-and-authorization)
    + [LDAP User Authentication](#ldap-user-authentication)
    + [User Rate Limits and Bandwidth Limits](#user-rate-limits-and-bandwidth-limits)
    + [User Blocking](#user-blocking)
- [Advanced Usage](#advanced-usage)
    + [mTLS Client](#mtls-client)
    + [Guomi TLCP Offloading](#guomi-tlcp-offloading)
    + [Multiplexing Multiple Protocols on One Port](#multiplexing-multiple-protocols-on-one-port)
    + [Listening on Multiple Ports](#listening-on-multiple-ports)
    + [Enabling PROXY Protocol on a Listening Port](#enabling-proxy-protocol-on-a-listening-port)
    + [Guomi TLCP Encapsulation](#guomi-tlcp-encapsulation)
    + [SOCKS5 UDP IP Mapping](#socks5-udp-ip-mapping)
    + [Secure Reverse Proxy](#secure-reverse-proxy)
    + [DNS Resolution Overrides](#dns-resolution-overrides)
    + [Dynamic Route Binding](#dynamic-route-binding)
    + [Dynamic Proxy Chaining](#dynamic-proxy-chaining)
    + [Per-User Site Monitoring](#per-user-site-monitoring)
    + [Per-User Site TLS MITM Configuration](#per-user-site-tls-mitm-configuration)
    + [Traffic Auditing](#traffic-auditing)
    + [Exporting Decrypted TLS Traffic](#exporting-decrypted-tls-traffic)
    + [Task Idle Detection](#task-idle-detection)
    + [Performance Optimization](#performance-optimization)
- [Scenario Design](#scenario-design)
    + [Multi-Region Acceleration](#multi-region-acceleration)
    + [Dual-Exit Failover](#dual-exit-failover)

## Installation

vey-proxy currently supports Linux only. Packaging is supported for distributions such as Debian and RHEL. After
building a package by following the [Release and Packaging Steps](/doc/build_and_package.md), install it directly on
the target system.

## Basic Concepts

### Service Management

You can deploy multiple vey-proxy services on a single host and manage them through systemd template units. Each
instance corresponds to one vey-proxy process group (`daemon_group`), and each process group exposes a Unix socket for
local RPC management.

Each service has one entry configuration file in YAML format. The file suffix can be changed, but all referenced
configuration files must use the same suffix. In this guide, *main.yml* refers to the entry configuration file.

If you install from the native distribution package, the systemd templated service file is installed automatically. The
template parameter is the process group name, and the entry configuration file is located at
`/etc/vey-proxy/<daemon_group>/main.yml`.

If you install without using a package, see [vey-proxy@.service](debian/vey-proxy@.service) and design your own
service management workflow.

### Hot Upgrades

The default systemd service configuration supports hot upgrades:

1. Install the new package.
2. Run `systemctl daemon-reload` to reload the updated unit files.
3. Run `systemctl restart vey-proxy@<daemon_group>` to start the new process and tell the old process to drain.

After the old process starts draining, it waits for existing tasks to finish, or it is forced offline after a timeout
(10 hours by default).

This works similarly to `nginx reload`. Because of operating system behavior, there is still a chance that new
connections may be dropped when sockets are released. Linux 5.14 and later introduced the
[tcp_migrate_req](https://docs.kernel.org/networking/ip-sysctl.html) option, which can prevent those drops.

### Configuration Structure

vey-proxy uses a modular design. The main functional modules are:

1. Server

   Accepts and processes client requests. It can call into the Escaper, UserGroup, and Auditor modules.
   A *Port* server can be chained in front of a non-port server.

2. Escaper

   Connects to and controls the target address. It can call into the Resolver module.
   A *Route* escaper can be chained in front of other escapers.

3. Resolver

   Provides DNS resolution.
   A *Failover* resolver can be chained in front of other resolvers.

4. UserGroup

   Provides authentication and authorization.

5. Auditor

   Provides traffic auditing.

These modules can be defined together in *main.yml*, or split into separate configuration files. Split files can be
reloaded independently.

Settings outside those modules, such as threads, logging, and monitoring, must still be defined in *main.yml*.

For a single-file example, see [examples/inspect_http_proxy](examples/inspect_http_proxy). For a split-file example,
see [examples/hybrid_https_proxy](examples/hybrid_https_proxy).

The examples below show only the relevant fragments, not full configurations. For complete examples, see
[examples](examples).

### Monitoring

To integrate with different observability stacks, the VEY project uses [StatsD](https://www.datadoghq.com/blog/statsd/)
as its metrics output protocol. You can choose any StatsD implementation that fits your environment, such as
[gostatsd](https://github.com/atlassian/gostatsd), then connect that to your own monitoring system.

Configure monitoring for vey-proxy in *main.yml*:

```yaml
stat:
  target:
    udp: 127.0.0.1:8125 # StatsD UDP socket
    # unix: /run/statsd.sock
  prefix: vey-proxy     # Metric prefix, for example server.task.total becomes vey-proxy.server.task.total
  emit_duration: 200ms  # Emission interval
```

Metric definitions are under [metrics](../sphinx/vey-proxy/metrics). Generating the Sphinx HTML documentation makes
them easier to browse.

## Basic Usage

### HTTP Proxy

To enable an HTTP proxy entry point, add an `HttpProxy` server:

```yaml
server:
  - name: http       # Must be unique; used by logs and metrics
    escaper: default # Required; can point to any escaper type
    type: http_proxy
    listen:
      address: "[::]:8080"
    tls_client: { }  # Enables layer-7 HTTPS forward support
```

### SOCKS Proxy

To enable a SOCKS proxy entry point, add a `SocksProxy` server:

```yaml
server:
  - name: socks        # Must be unique; used by logs and metrics
    escaper: default   # Required; can point to any escaper type
    type: socks_proxy
    listen:
      address: "[::]:10086"
    enable_udp_associate: true # Use standard UDP Associate; otherwise use simplified UDP Connect (single peer only)
    udp_socket_buffer: 512K    # Client-side bidirectional UDP socket buffer size
```

### TCP Mapping

To map a local TCP port to a specific port on the target host, add a `TcpStream` server:

```yaml
server:
  - name: tcp           # Must be unique; used by logs and metrics
    escaper: default    # Required; can point to any escaper type
    type: tcp_stream
    listen:
      address: "[::1]:10086"
    proxy_pass: # One or more target addresses
      - "127.0.0.1:5201"
      - "127.0.0.1:5202"
    upstream_pick_policy: rr # Load-balancing policy; default is random
```

### TLS Offloading

To map a local TCP port to a TLS port on the target host, use a `TcpStream` server:

```yaml
server:
  - name: tcp           # Must be unique; used by logs and metrics
    escaper: default    # Required; can point to any escaper type
    type: tcp_stream
    listen: "[::1]:80"
    proxy_pass: "127.0.0.1:443"
    tls_client: { }     # Use TLS to connect upstream; configure CA, client certs (mTLS), and so on
```

### TLS Encapsulation

To map a local TLS port to a specific port on the target host, add a `TlsStream` server:

```yaml
server:
  - name: tls           # Must be unique; used by logs and metrics
    escaper: default    # Required; can point to any escaper type
    type: tls_stream
    listen:
      address: "[::1]:10443"
    tls_server: # TLS settings
      cert_pairs:
        certificate: /path/to/cert
        private_key: /path/to/key
      enable_client_auth: true # Optional: enable mTLS
    proxy_pass: # One or more target addresses
      - "127.0.0.1:5201"
      - "127.0.0.1:5202"
    upstream_pick_policy: rr # Load-balancing policy; default is random
```

You can also chain a `PlainTlsPort` in front of a `TcpStream` server:

```yaml
server:
  - name: tcp
    escaper: default
    type: tcp_stream
    proxy_pass:
      - "127.0.0.1:5201"
      - "127.0.0.1:5202"
    upstream_pick_policy: rr
  - name: tls
    type: plain_tls_port
    listen:
      address: "[::1]:10443"
    tls_server:
      cert_pairs:
        certificate: /path/to/cert
        private_key: /path/to/key
      enable_client_auth: true # Optional: enable mTLS
    server: tcp # Forward to the tcp_stream server
```

### SNI Proxy

To detect the target automatically from the TLS SNI or HTTP `Host` header and forward the connection, add an
`SniProxy` server:

```yaml
server:
  - name: sni          # Must be unique; used by logs and metrics
    escaper: default   # Required; can point to any escaper type
    type: sni_proxy
    listen:
      address: "[::]:443" # Can handle both TLS and HTTP traffic on port 443
```

### Transparent Proxy

On a gateway device, you can redirect the TCP connections that need proxying to a `TcpTProxy` server so the proxy can
forward them transparently:

```yaml
server:
  - name: transparent
    escaper: default
    auditor: default  # Needed for protocol inspection, TLS interception, and similar features
    type: tcp_tproxy
    listen: "127.0.0.1:1234"
```

The required system configuration depends on the operating system:

- Linux [TPROXY](https://docs.kernel.org/networking/tproxy.html)
- FreeBSD [ipfw fwd](https://man.freebsd.org/cgi/man.cgi?query=ipfw)
- OpenBSD [pf divert-to](https://man.openbsd.org/pf.conf.5#divert-to)

### Route Binding

If a machine has multiple network paths and you need to force outbound traffic onto one of them, set a bind IP on the
escaper. Using `DirectFixed` as an example:

```yaml
escaper:
  - name: default
    type: direct_fixed
    resolver: default
    resolve_strategy: IPv4First # Happy Eyeballs is supported; prefer IPv4 when resolving
    bind_ip: 192.168.10.1       # You can use a list to set multiple addresses
resolver:
  - name: default
    type: c-ares
    server: 223.5.5.5
    bind_ipv4: 192.168.10.1     # Bind DNS queries to the same path to keep resolution local
```

### Proxy Chaining

If you need to forward traffic through another proxy, use a *Proxy* escaper. `ProxyHttps` is shown below:

```yaml
escaper:
  - name: next_proxy
    type: proxy_https
    resolver: default   # Required when proxy_addr contains a domain name
    proxy_addr: next-proxy.example.net:8443 # You can also provide multiple proxy addresses
    http_forward_capability:
      forward_ftp: true   # Forward FTP-over-HTTP requests to the next proxy instead of handling FTP locally
      forward_https: true # Forward HTTPS CONNECT traffic to the next proxy instead of doing the TLS handshake locally
    tls_client:
      ca_certificate: rootCA.pem # CA used to verify the next proxy; defaults to the system CA store
    tls_name: example.com # Required for DNS-name validation if proxy_addr does not contain a domain name
```

#### Username Parameters to Derive the Next-Hop Address

For HTTP and SOCKS5 proxy servers, you can append ordered key-value pairs to the username and use them to derive the
next-hop proxy address dynamically: `base-key1-val1-key2-val2-...`.

Example:

```yaml
server:
  - name: http-in
    type: http_proxy
    escaper: chain
    username_params: # All keys are added to the egress context
      required_keys: host
      optional_keys: session-id
      param_separator: '_'

escaper:
  - name: comply_http
    type: comply_context # Extract values from the egress context and set the dynamic next-hop address
    next: proxy_http
    use_egress_upstream:
      default_port: 8080
      host_key: host
      domain_suffix: example.net
      resolve_sticky_key: session-id
  - name: proxy_http
    type: proxy_http # Uses the dynamic address set by comply_http
    proxy_addr: 127.0.0.1:3128 # Default address
```

The client can then connect with:

`http://my_name-host-proxy1-session_id-1234:password@xxx`

The actual next-hop proxy becomes `proxy1.example.net`, and sticky resolution uses `1234` as its sticky key.

### Connection Rate Limits

Per-connection bandwidth limits are supported on the `server`, `escaper`, and `user` levels. The configuration keys are
the same in all three places:

```yaml
tcp_sock_speed_limit: 10M/s
udp_sock_speed_limit: 10M/s
```

For `server` and `user`, the limits apply to client-to-proxy connections. For `escaper`, they apply to
proxy-to-target connections.

### Process-Wide Rate Limits

User configuration also supports process-wide bandwidth limits:

```yaml
tcp_all_download_speed_limit: 100M/s
tcp_all_upload_speed_limit: 100M/s
udp_all_download_speed_limit: 100M/s
udp_all_upload_speed_limit: 100M/s
```

### DNS Resolution

To use the system default `/etc/resolv.conf`:

```yaml
resolver:
  - name: default
    type: c-ares
```

To use specific DNS servers:

```yaml
resolver:
  - name: c-ares
    type: c-ares
    server:
      - 1.1.1.1
      - 1.0.0.1
  - name: hickory
    type: hickory
    server:
      - 8.8.8.8
      - 8.8.4.4
```

### Secure DNS Resolution

If you need encrypted access to upstream recursive DNS servers, use the `hickory` resolver:

```yaml
resolver:
  - name: default
    type: hickory
    server: 1.1.1.1
    encryption: dns-over-https # Also supports dns-over-tls, dns-over-quic, and dns-over-h3
```

### Failover DNS Resolution

If a single upstream recursive DNS server is unreliable, use a `Failover` resolver:

```yaml
resolver:
  - name: virtual
    type: fail_over
    primary: alidns
    standby: dnspod
  - name: alidns
    type: c-ares
    server: 223.5.5.5 223.6.6.6
  - name: dnspod
    type: c-ares
    server: 119.29.29.29
```

### User Authentication and Authorization

Both HTTP proxy and SOCKS5 proxy support user authentication. This requires a `UserGroup` configuration. For a complete
example, see [examples/simple_user_auth](examples/simple_user_auth). A sample user group is shown below:

```yaml
user_group:
  - name: default
    static_users:
      - name: root
        # password: toor
        token: # Authentication token
          salt: 113323bdab6fd2cc
          md5: 5c81f2becadde7fa5fde9026652ccc84
          sha1: ff9d5c1a14328dd85ee95d4e574bd0558a1dfa96
        dst_port_filter: # Allowed ports
          - 80
          - 443
        dst_host_filter_set: # Allowed destinations
          exact:
            - ipinfo.io          # Allow access to ipinfo.io
            - 1.1.1.1
          child:
            - "ipip.net"         # Allow access to myip.ipip.net
          regex:
            - "lum[a-z]*[.]com$" # Allow access to lumtest.com
    source: # Dynamic users; static users take precedence
      type: file                # Can also be loaded and cached through Lua or Python scripts
      path: dynamic_users.json
```

Use [scripts/passphrase_hash.py](/scripts/passphrase_hash.py) to generate the authentication token fields.

### LDAP User Authentication

User password verification can also be delegated to a remote LDAP service:

```yaml
user_group:
  - name: default
    type: ldap
    ldap_url: ldap://ldap.forumsys.com/dc=example,dc=com
    pool:
      min_idle_count: 1
    static_users:
      - name: gauss
        # No token is required; this user can still have explicit configuration
    source: # Dynamic user configuration
      type: file
      path: dynamic_users.json
    unmanaged_user: # Template for users that pass LDAP auth but have no local config
      name: unmanaged
```

### User Rate Limits and Bandwidth Limits

At the user level, you can enforce per-connection bandwidth limits, request-rate limits, and concurrency limits:

```yaml
tcp_sock_speed_limit: 10M/s # 10M/s in each direction for a single TCP connection
udp_sock_speed_limit: 10M/s # 10M/s in each direction for a single UDP connection
tcp_conn_rate_limit: 1000/s # Rate limit for new client-to-proxy TCP connections
request_rate_limit: 2000/s  # Rate limit for new proxy requests
request_max_alive: 2000     # Maximum number of active tasks
```

### User Blocking

Deleting a user does not terminate that user's existing tasks by default. To terminate them, mark the user as blocked.
Existing tasks will be cleaned up within at most two [Task Idle Detection](#task-idle-detection) intervals.

Configure a blocked user like this:

```yaml
- name: foo
  block_and_delay: 1s # Block the user and delay new responses by the configured duration
  # Other settings can remain unchanged
```

## Advanced Usage

### mTLS Client

Several examples in this guide refer to `tls_client`. To enable mutual TLS as a client, use:

```yaml
tls_client:
  certificate: /path/to/cert.crt     # Client certificate
  private_key: /path/to/pkey.key     # Client private key
  ca_certificate: /path/to/ca/cert.crt # CA used to verify the server certificate; defaults to the system CA store
```

### Guomi TLCP Offloading

This feature requires the `vendored-tongsuo` feature to be enabled at build time.

Some environments require the Guomi protocol, but many clients do not support it. vey-proxy can translate between
protocols:

* TLCP to layer-4 TCP

```yaml
server:
  - name: l4tcp
    type: tcp_stream
    listen: "[::1]:10086"
    upstream: "127.0.0.1:443" # Remote Guomi server address; domains are supported
    tls_client:
      protocol: tlcp
      ca_certificate: /path/to/ca.cert # CA certificate
      # You can also add mTLS settings here
    upstream_tls_name: target.host.domain # Used for peer verification; optional if upstream already uses a domain
```

* TLCP to layer-4 TLS

```yaml
server:
  - name: l4tls
    type: tls_stream
    tls_server:
      cert_pairs:
        - certificate: /path/to/cert
          private_key: /path/to/key
    # Other settings are the same as the tcp_stream example above
```

* TLCP to layer-7 HTTP

```yaml
server:
  - name: l7http
    type: http_rproxy
    listen: "[::1]:80"
    hosts:
      - set_default: true
        upstream: "127.0.0.1:443"
        tls_client:
          protocol: tlcp
          ca_certificate: /path/to/ca.cert
          # You can also add mTLS settings here
        tls_name: target.host.domain # Used for peer verification; optional if upstream already uses a domain
```

* TLCP to layer-7 HTTPS

```yaml
server:
  - name: l7http
    type: http_rproxy
    listen: "[::1]:443"
    hosts:
      - set_default: true
        upstream: "127.0.0.1:443"
        tls_client:
          protocol: tlcp
          ca_certificate: /path/to/ca.cert
          # You can also add mTLS settings here
        tls_name: target.host.domain # Used for peer verification; optional if upstream already uses a domain
        tls_server: # TLS configuration for this host
          cert_pairs:
            - certificate: /path/to/cert
              private_key: /path/to/key
    enable_tls_server: true
    # global_tls_server can define the default TLS configuration for hosts that do not set tls_server
```

### Multiplexing Multiple Protocols on One Port

If you want a single port to accept both `HttpProxy` and `SocksProxy`, use an `IntelliProxy` port:

```yaml
server:
  - name: intelli
    type: intelli_proxy
    listen: "[::]:8080"
    http_server: http        # HTTP requests go to the http server
    socks_server: socks      # SOCKS requests go to the socks server
  - name: http
    type: HttpProxy
    listen: "127.0.0.1:2001" # Bind locally to prevent direct external use
  - name: socks
    type: SocksProxy
    listen: "127.0.0.1:2002" # Bind locally to prevent direct external use
```

### Listening on Multiple Ports

If the same service needs to listen on multiple ports, chain a *Port* server in front of it.

Example: make `SniProxy` listen on both 443 and 80:

```yaml
server:
  - name: sni
    escaper: default
    type: sni_proxy
    listen:
      address: "[::]:443"
  - name: port80
    type: plain_tcp_port
    listen: "[::]:80"
    server: sni_proxy
```

Example: expose both plaintext and TLS ports for an HTTP proxy:

```yaml
server:
  - name: http
    escaper: default
    type: http_proxy
    listen: "[::]:8080"
    tls_client: { }
  - name: tls
    type: plain_tls_port
    listen: "[::]:8443"
    server: http
    tls_server:
      cert_pairs:
        certificate: /path/to/certificate
        private_key: /path/to/private_key
      enable_client_auth: true # Optional: enable mTLS
```

Port-type servers have their own listener metrics only. Traffic metrics and logs are emitted by the next-hop server, so
choose between port chaining and separate servers based on how you want to observe the service.

### Enabling PROXY Protocol on a Listening Port

In a chained deployment, if you need to preserve the original client address, use the PROXY Protocol. `PlainTcpPort`
and `PlainTlsPort` can expose dedicated ports that accept PROXY Protocol:

```yaml
server:
  - name: real_http
    listen: "[127.0.0.1]:1234" # Optional
    type: http_proxy
    ingress_network_filter: { } # Filter for the source address carried in the PROXY header
    # ... other settings
  - name: pp_for_http
    type: plain_tcp_port
    listen: "[::]:8080"
    server: real_http
    proxy_protocol: v2
    ingress_network_filter: { } # Filter for the original peer socket address
```

### Guomi TLCP Encapsulation

This feature requires the `vendored-tongsuo` feature to be enabled at build time.

You can use `NativeTlsPort` to encapsulate Guomi TLCP:

```yaml
server:
  - name: real_http
    listen: "[127.0.0.1]:1234" # Optional
    type: http_proxy
    # ... other settings
  - name: tlcp
    type: native_tls_port
    listen: "[::]:443"
    tls_server:
      tlcp_cert_pairs: # Enables Guomi TLCP
        sign_certificate: /path/to/sign.crt
        sign_private_key: /path/to/sign.key
        enc_certificate: /path/to/enc.crt
        enc_private_key: /path/to/enc.key
      enable_client_auth: true # Optional: enable mTLS
    server: real_http
    proxy_protocol: v2         # Optional: enable PROXY Protocol
```

### SOCKS5 UDP IP Mapping

When handling SOCKS5 UDP, the server must tell the client which address to use for the UDP data channel. Normally this
is the proxy's local `IP:Port`, but sometimes the client cannot reach that local IP directly. In that case, configure a
mapping table on the SOCKS server:

```yaml
transmute_udp_echo_ip:
  "192.168.10.2": "192.168.30.2"
```

### Secure Reverse Proxy

Many applications expose HTTP APIs or metrics endpoints with only minimal built-in protection. The following pattern can
be used to harden them:

```yaml
server:
  - name: plain
    escaper: default
    user-group: default                     # Enable user authentication
    type: http_rproxy
    listen:
      address: "[::]:80"
    no_early_error_reply: true              # Do not return errors until the request is validated; helps resist port scans
    hosts:
      - exact_match: service1.example.net   # Match this hostname
        upstream: 127.0.0.1:8081            # Forward all paths
      - exact_match: service2.example.net   # Match this hostname
        set_default: true                   # Use as the default site if no hostname matches
        upstream: 127.0.0.1:8082            # Forward all paths
    # You can enable TLS with tls_server, or add a separate TLS port through a fronting plain_tls_port
```

### DNS Resolution Overrides

Sometimes you need to bypass normal DNS resolution and apply custom name resolution rules. You can do that in the user
configuration:

```yaml
resolve_redirection:
  - exact: t1.example.net # Force to a specific IP
    to: 192.168.10.1
  - exact: t2.example.net # CNAME-style rewrite
    to: t1.example.net
  - child: example.com    # Rewrite *.example.com to *.example.net
    to: example.net
```

### Dynamic Route Binding

Some machines get their IPs dynamically, for example through DHCP or PPP. Those addresses can be published into a
`DirectFloat` escaper at runtime.

Proxy configuration:

```yaml
escaper:
  - name: float
    type: direct_float
    resolver: default
```

Publish an updated address with:

```shell
vey-proxy-ctl -G <daemon_group> -p <pid> escaper float publish "{\"ipv4\": \"192.168.10.1\"}"
```

### Dynamic Proxy Chaining

In crawling scenarios, upstream proxy addresses are often short-lived. You can put a stable intermediary proxy in front
and let a helper process keep updating the real upstream proxy when it expires. Clients then only need one fixed proxy
address.

Proxy configuration:

```yaml
escaper:
  - name: float
    type: proxy_float
    source:
      type: passive   # Accept pushed updates; can also be configured to read periodically from Redis
```

Publish an updated upstream proxy with:

```shell
vey-proxy-ctl -G <daemon_group> -p <pid> escaper float publish '{"type":"socks5","addr":"127.0.0.1:11080", "expire": "<rfc3339 datetime>"}'
```

The `type` field can also be `http` or `https`.

### Per-User Site Monitoring

Within a user configuration, you can define site-specific rules and attach independent metrics or settings:

```yaml
explicit_sites:
  - id: example-net
    child_match: example.net
    emit_stats: true # Emit separate metrics; id becomes part of the metric name
    resolve_strategy:
      query: ipv4only # Resolve IPv4 only
```

### Per-User Site TLS MITM Configuration

Within a user-site rule, you can customize how the TLS client behaves when TLS interception is enabled:

```yaml
explicit_sites:
  - id: example-net
    child_match: example.net
    tls_client:
      ca_certificate: xxx      # PEM CA certificate
      cert_pairs:
        certificate: xxx       # PEM client certificate
        private_key: xxx       # PEM client private key
      # Other tls_client settings
```

### Traffic Auditing

For a complete example of traffic auditing, see [examples/inspect_http_proxy](examples/inspect_http_proxy). A typical
auditor configuration looks like this:

```yaml
auditor:
  - name: default
    protocol_inspection: { }      # Enable protocol detection with default settings
    tls_cert_generator: { }       # Enable TLS interception with default settings; peer defaults to 127.0.0.1:2999
    tls_interception_client: { }  # Optional TLS client settings for upstream connections made during interception
    h1_interception: { }          # HTTP/1.0 parsing settings
    h2_interception: { }          # HTTP/2 parsing settings
    icap_reqmod_service: icap://xxx  # ICAP REQMOD service
    icap_respmod_service: icap://xxx # ICAP RESPMOD service
    application_audit_ratio: 1.0     # Fraction of application traffic to audit
```

This feature requires a TLS certificate generator. A reference implementation is [vey-dcgen](/vey-dcgen); see
[vey-dcgen simple conf](/vey-dcgen/examples/simple) for an example configuration.

### Exporting Decrypted TLS Traffic

If traffic auditing and TLS interception are both enabled, you can export decrypted TLS traffic to
[udpdump](https://www.wireshark.org/docs/man-pages/udpdump.html).

For the detailed configuration, see [examples/inspect_http_proxy](examples/inspect_http_proxy).

### Task Idle Detection

Every successful task can exit automatically after being idle for too long. Two settings control this behavior: the
idle check interval and the allowed idle count. Both can be configured on the server:

```yaml
- name: foo
  type: xxx                    # Applies to any server type
  task_idle_check_interval: 1m # Default is 1 minute
  task_idle_max_count: 5       # Default maximum is 5; the task is terminated when the count is reached
```

The allowed idle count can also be set per user, overriding the server setting:

```yaml
- name: foo
  task_idle_max_count: 5
```

### Performance Optimization

By default, the proxy uses all CPU cores and may schedule work across cores. In some environments, pinning workers to
specific CPUs improves performance.

Configure workers in *main.yml*:

```yaml
worker:
  thread_number: 8      # Defaults to the full CPU core count if omitted
  sched_affinity: true  # Pin workers to CPUs in order by default; you can also provide an explicit worker-to-CPU map
```

When configuring a listener, you can also make it listen separately in each worker:

```yaml
listen: "[::]:8080"
listen_in_worker: true
```

## Scenario Design

### Multi-Region Acceleration

You can combine existing vey-proxy modules to build cross-region acceleration.

For a three-region deployment, the topology looks like this:

```mermaid
flowchart LR
%% Paste to https://mermaid.live/ to see the graph
    subgraph Area1
        a1_client[Client]
        a1_site[Site]
        subgraph Proxy1
            a1_proxy[GW]
            a1_relay[relay]
            a1_route[route]
            a1_proxy -.-> a1_route
        end
        a1_client --> a1_proxy
        a1_route -- local --> a1_site
        a1_relay -- local --> a1_site
    end
    subgraph Area2
        a2_client[Client]
        a2_site[Site]
        subgraph Proxy2
            a2_proxy[GW]
            a2_relay[relay]
            a2_route[route]
            a2_proxy -.-> a2_route
        end
        a2_client --> a2_proxy
        a2_route -- local --> a2_site
        a2_relay -- local --> a2_site
    end
    subgraph Area3
        a3_client[Client]
        a3_site[Site]
        subgraph Proxy3
            a3_proxy[GW]
            a3_relay[relay]
            a3_route[route]
            a3_proxy -.-> a3_route
        end
        a3_client --> a3_proxy
        a3_route -- local --> a3_site
        a3_relay -- local --> a3_site
    end
    a1_route -- mTLS to a2 ----> a2_relay
    a1_route -- mTLS to a3 ----> a3_relay
    a2_route -- mTLS to a1 ----> a1_relay
    a2_route -- mTLS to a3 ----> a3_relay
    a3_route -- mTLS to a1 ----> a1_relay
    a3_route -- mTLS to a2 ----> a2_relay
```

Each node's proxy typically has the following roles:

- GW

  Handles local user requests. For layer-4 acceleration you can use [SNI Proxy](#sni-proxy); for layer-7 acceleration
  you can use [Secure Reverse Proxy](#secure-reverse-proxy).

  Minimal example:

  ```yaml
  server:
    - name: port443
      type: sni_proxy
      escaper: route
    - name: port80
      type: http_rproxy
      escaper: route
  ```

- relay

  Handles requests from other regions over an internal protocol, such as mTLS.

  Minimal example:

  ```yaml
  server:
    - name: relay
      type: http_proxy
      escaper: local
      tls_server: {} # Configure TLS settings
  ```

- route

  Chooses the path for local user requests. You need at least one route-type escaper, one local escaper, and one proxy
  escaper for each remote region.

  Minimal example:

  ```yaml
  escaper:
    - name: route
      type: route_query  # Can query an external agent for routing, or you can use another route escaper
      query_allowed_next:
        - a1_proxy
        - a2_proxy
        - local
      fallback_node: local
      # ... agent configuration
    - name: local
      type: direct_fixed
      # ... local escaper configuration
    - name: a1_proxy
      type: proxy_https
      tls_client: {} # Configure TLS settings
      # ... point this at the relay endpoint in area a1
    - name: a2_proxy
      type: proxy_https
      tls_client: {} # Configure TLS settings
      # ... point this at the relay endpoint in area a2
  ```

### Dual-Exit Failover

If a single IDC has multiple public egress POPs, or any comparable setup with at least two **non-local** network paths
to the target site, you can build automatic primary/standby failover like this:

```mermaid
flowchart LR
%% Paste to https://mermaid.live/ to see the graph
    subgraph IDC
        i1_client[Client]
        subgraph Proxy
            i1_proxy[GW]
            i1_route[route]
            i1_proxy -.-> i1_route
        end
        i1_client --> i1_proxy
    end
    subgraph POP1
        p1_proxy[relay]
    end
    subgraph POP2
        p2_proxy[relay]
    end
    internet[Internet]
    i1_route -- proxy to pop1 --> p1_proxy
    i1_route -- proxy to pop2 --> p2_proxy
    p1_proxy -- local ---> internet
    p2_proxy -- local ---> internet
```

Each node's proxy typically has the following roles:

- GW

  Handles client requests. It can be any server type, such as a forward proxy, reverse proxy, or TCP mapping service.

- relay

  Handles requests from other nodes over an internal protocol, such as mTLS.

  Minimal example:

  ```yaml
  server:
    - name: relay
      type: http_proxy
      escaper: local
      tls_server: {} # Configure TLS settings
  ```

  If the GW inside the IDC must support SOCKS5 UDP, then the relay should also be a UDP-capable proxy. In that case,
  use [SOCKS Proxy](#socks-proxy).

- route

  Chooses the path for local user requests. You need at least one route-type escaper plus one proxy escaper for each
  POP.

  Minimal example:

  ```yaml
  escaper:
    - name: route
      type: route_failover
      primary_next: p1_proxy
      standby_next: p2_proxy
      fallback_delay: 100ms  # Start the standby attempt after this delay if the primary has not succeeded
    - name: p1_proxy
      type: proxy_https # Must match the relay server type used by POP1
      tls_client: {} # Configure TLS settings
      # ... point this at the relay endpoint in POP1
    - name: p2_proxy
      type: proxy_https # Must match the relay server type used by POP2
      tls_client: {} # Configure TLS settings
      # ... point this at the relay endpoint in POP2
  ```
