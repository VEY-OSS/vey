.. _configuration_server_http_guard:

http_guard
==========

This server is the public-edge HTTP reverse proxy. Incoming TCP is inspected
before any site lookup: HTTP/1.x is accepted, and HTTP/2 is accepted over TLS
(ALPN ``h2``) or as plaintext H2C when enabled. TLS is detected automatically
(including TLCP). Other protocols are dropped.

TLS connections match SNI and later ``Host`` against sites that have
``tls_server``. Plaintext connections match ``Host`` against the HTTP host
table. After a TLS handshake, ``Host`` still uses the TLS-capable table.

It then forwards requests to that site's origin. HTTP/2 origin stays on
HTTP/2 (no HTTP/1 fallback). There is no visitor authentication; tenant
identity comes from ``site.owner`` plus the group's ``tenant_user_group``.

This is the counterpart of :ref:`http_expose <configuration_server_http_rproxy>`
(internal reverse proxy with optional visitor auth and no auditor).

It supports:

* HTTP/1.0 and HTTP/1.1
* HTTP/2, including RFC 8441 WebSocket (extended ``CONNECT``)
* HTTP/1 WebSocket upgrades (``Upgrade: websocket``)
* optional H2C (plaintext HTTP/2), off by default
* optional ICAP via :ref:`auditor <conf_server_common_auditor>` (REQMOD / RESPMOD)

It does **not** support standard ``CONNECT`` (without ``:protocol``). HTTP/3
is not enabled.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tls ticketer <conf_server_common_tls_ticketer>`
* :ref:`tcp_sock_speed_limit <conf_server_common_tcp_sock_speed_limit>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`tcp_copy_buffer_size <conf_server_common_tcp_copy_buffer_size>`
* :ref:`tcp_copy_yield_size <conf_server_common_tcp_copy_yield_size>`
* :ref:`tcp_misc_opts <conf_server_common_tcp_misc_opts>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
* :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
* :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`
* :ref:`site_group <config_server_http_guard_site_group>`

``user_group`` is rejected. Do not configure visitor authentication on this
server.

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

.. _config_server_http_guard_server_id:

server_id
---------

**optional**, **type**: :external+values:ref:`http server id <conf_value_http_server_id>`

Server ID. If set, the value is used in the ``Via`` header added to requests
and as the RFC 9209 ``Proxy-Status`` identifier on locally generated error
responses. If unset, those error responses use ``vey-proxy`` as the identifier.

HTTP errors generated during ICAP adaptation use the auditor
:ref:`server_id <conf_auditor_server_id>`, not this value.

See :ref:`protocol_client_proxy_status`.

**default**: not set

.. _config_server_http_guard_no_proxy_status:

no_proxy_status
---------------

**optional**, **type**: bool

If set to ``true``, locally generated HTTP error responses do not include an
RFC 9209 ``Proxy-Status`` header.

**default**: false

req_header_recv_timeout
-----------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Maximum time to wait for the full request header after the client connection
becomes readable.

**default**: 30s

rsp_header_recv_timeout
-----------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Maximum time to wait after the full request is sent and before the full
response header is received.

**default**: 60s

req_header_max_size
-------------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Maximum request-header size.

**default**: 64KiB

rsp_header_max_size
-------------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Maximum response-header size.

**default**: 64KiB

log_uri_max_chars
-----------------

**optional**, **type**: usize

Maximum number of URI characters recorded in logs.

A tenant user ``log_uri_max_chars`` value overwrites this when the site has an
owner.

**default**: 1024

no_early_error_reply
--------------------

**optional**, **type**: bool

If set to ``true``, protocol-parse errors close the connection without writing
an HTTP error response.

**default**: false

append_forwarded_for
--------------------

**optional**, **type**: :external+values:ref:`http forwarded header type <conf_value_http_forwarded_header_type>`

How the client address is added to forwarded requests.

**default**: disable

h1
--

**optional**, **type**: map

HTTP/1-only settings.

pipeline_size
^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`nonzero usize <conf_value_nonzero_usize>`

Pipeline depth for HTTP/1.0 and HTTP/1.1.

**default**: 10

.. note::

  We only pipeline requests with no body. WebSocket upgrades take the client
  reader for the rest of the connection.

pipeline_read_idle_timeout
^^^^^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Idle timeout for client-side idle HTTP connections.

**default**: 5min

body_line_max_length
^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: int

Maximum line length for lines in the HTTP body, such as trailer fields and
chunk-size lines.

**default**: 8192

http_forward_upstream_keepalive
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`http keepalive <conf_value_http_keepalive>`

HTTP keepalive configuration at the server level. Site
:ref:`h1 connection_pool <conf_site_http_h1_connection_pool>` still applies
when configured.

**default**: set with default value

h2
--

**optional**, **type**: map

HTTP/2-only settings.

enable_h2c
^^^^^^^^^^

**optional**, **type**: bool

Accept plaintext HTTP/2 (H2C) on the listen port. TLS clients still negotiate
HTTP/2 via ALPN regardless of this key.

**default**: false

max_header_list_size
^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize u32 <conf_value_humanize_u32>`

Maximum HTTP/2 header list size.

**default**: 64KiB

max_concurrent_streams
^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: u32

Maximum concurrent streams initiated by the client.

**default**: 128

max_frame_size
^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize u32 <conf_value_humanize_u32>`

Maximum HTTP/2 frame size.

**default**: 256KiB

stream_window_size
^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize u32 <conf_value_humanize_u32>`

Initial stream window size.

**default**: 1MiB

connection_window_size
^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize u32 <conf_value_humanize_u32>`

Connection window size.

**default**: 2MiB

max_send_buffer_size
^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Maximum send buffer size per stream.

**default**: 8MiB

upstream_handshake_timeout
^^^^^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for the origin HTTP/2 handshake.

**default**: 10s

upstream_stream_open_timeout
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout to open a stream on a pooled origin HTTP/2 connection.

**default**: 10s

client_handshake_timeout
^^^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for the client HTTP/2 handshake.

**default**: 4s

ping_interval
^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Interval for origin HTTP/2 PING. ``0`` disables PING.

**default**: 60s

.. _configuration_server_http_guard_global_tls_server:

global_tls_server
-----------------

**optional**, **type**: :external+values:ref:`openssl server config <conf_value_openssl_server_config>`

Global TLS server configuration used when the matched site does not set
its own TLS server configuration.

**default**: not set

client_hello_recv_timeout
-------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for receiving the complete TLS ClientHello message.

**default**: 1s

.. _config_server_http_guard_site_group:

site_group
----------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Name of the :ref:`site group <configuration_site_group>` that provides Host /
SNI matching and per-site upstream settings.

If the referenced group does not exist, an empty group is used and no site
matches.

**default**: not set
