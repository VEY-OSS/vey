.. _configuration_server_http_expose:

http_expose
===========

This server is the internal HTTP reverse proxy. It terminates the client-side
HTTP session locally and then forwards requests to configured upstream sites
selected from the referenced ``site_group``. Whether the origin hop uses TLS
is decided by site :ref:`tls_client <conf_site_tls_client>`. The forward task
type is always ``HttpForward``; it is not ``HttpsForward``, which is the
``http_proxy`` ``https://`` request type.

It supports optional visitor authentication and HTTP/1 only. ``auditor`` is
rejected. The public-edge counterpart is
:ref:`http_guard <configuration_server_http_guard>` (no visitor auth,
optional ICAP, HTTP/2 over TLS).

A blocked visitor still cancels the current request. A blocked tenant is
rejected only at site entry for new requests; see
:ref:`block_and_delay <conf_auth_user_block_and_delay>`.

``type: http_rproxy`` is still accepted as a deprecated alias.

TLS SNI is used only to pick a certificate. Request routing always uses
``Host``. Unlike :ref:`http_guard <configuration_server_http_guard>`, this
server does not pin the TLS site or reject a mismatched ``Host`` with
``421``.

HTTP/1 request-target may be origin-form, or an absolute-form whose scheme is
``http`` or ``https``. Other schemes are rejected. An ``https://`` request-target
is still :ref:`HttpForward <log_task_http_forward>` and does not require the
client connection to be TLS.

.. versionchanged:: 1.15.0
   type renamed from ``http_rproxy`` (still accepted as a deprecated alias);
   origin keepalive moved to the site (``http.h1.upstream_keepalive``)

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`user_group <conf_server_common_user_group>`
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
* :ref:`site_group <config_server_http_expose_site_group>`

The authentication schemes supported by this server depend on the type of the
configured user group.

+-------------+---------------------------+-------------------+
|auth scheme  |user group type            |is supported       |
+=============+===========================+===================+
|Basic        |hashed_user                |yes                |
+-------------+---------------------------+-------------------+
|Negotiate    |gss_api                    |not yet            |
+-------------+---------------------------+-------------------+

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

.. versionadded:: 1.7.20 change listen config to be optional

.. _config_server_http_expose_server_id:

server_id
---------

**optional**, **type**: :external+values:ref:`http server id <conf_value_http_server_id>`

Server ID. If set, the ``X-VEY-Remote-Connection-Info`` header is added to
responses, and the value is also used in the ``Via`` header added to requests.
The same value is used as the RFC 9209 ``Proxy-Status`` identifier on locally
generated error responses. If unset, those error responses use ``vey-proxy`` as
the identifier.

HTTP errors generated during protocol interception use the auditor
:ref:`server_id <conf_auditor_server_id>`, not this value.

See :ref:`protocol_client_proxy_status`.

**default**: not set

.. _config_server_http_expose_no_proxy_status:

no_proxy_status
---------------

**optional**, **type**: bool

If set to ``true``, locally generated HTTP error responses do not include an
RFC 9209 ``Proxy-Status`` header.

HTTP errors generated during protocol interception use the auditor
:ref:`no_proxy_status <conf_auditor_no_proxy_status>`, not this value.

See :ref:`protocol_client_proxy_status`.

**default**: false

.. versionadded:: 1.15.0

auth_realm
----------

**optional**, **type**: :external+values:ref:`ascii str <conf_value_ascii_str>`

Authentication realm.

**default**: vey-proxy

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

.. _config_server_http_expose_log_uri_max_chars:

log_uri_max_chars
-----------------

**optional**, **type**: usize

Maximum number of URI characters recorded in logs.

The user level config value will take effect if set, see this :ref:`user config option <config_user_log_uri_max_chars>`.

**default**: 1024

pipeline_size
-------------

**optional**, **type**: :external+values:ref:`nonzero usize <conf_value_nonzero_usize>`

Pipeline depth for HTTP/1.0 and HTTP/1.1.

**default**: 10

.. note::

  We only pipeline requests with no body.

pipeline_read_idle_timeout
--------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Idle timeout for client-side idle HTTP connections.

**default**: 5min

no_early_error_reply
--------------------

**optional**, **type**: bool

If set to ``true``, no error response is sent before user authentication
succeeds. In that case the connection is simply closed.

**default**: false

body_line_max_length
--------------------

**optional**, **type**: int

Maximum line length for lines in the HTTP body, such as trailer fields and
chunk-size lines.

**default**: 8192

.. note::

   Origin HTTP/1 keepalive is configured on the site
   (:ref:`http.h1.upstream_keepalive <conf_site_http_h1_upstream_keepalive>`).
   The former server key ``http_forward_upstream_keepalive`` is rejected.

untrusted_read_speed_limit
--------------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Enables untrusted reading of request bodies that do not yet have authentication
information, and sets the corresponding read-rate limit.

The request Host is matched to a site before visitor authentication. Untrusted
drain is also limited by that site's (and tenant's)
:ref:`tcp_sock_speed_limit <conf_site_tcp_sock_speed_limit>`,
:ref:`request_rate_limit <conf_site_request_rate_limit>`,
:ref:`request_max_alive <conf_site_request_max_alive>`, and
:ref:`task_idle_max_count <conf_site_task_idle_max_count>`. A blocked tenant
forbids the request at site entry and does not drain.

Use this if you need compatibility with buggy Java HTTP clients that do not
handle ``407`` responses promptly.

**default**: not set, which means untrusted read is disabled

untrusted_read_limit
--------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use untrusted_read_speed_limit instead

append_forwarded_for
--------------------

**optional**, **type**: :external+values:ref:`http forwarded header type <conf_value_http_forwarded_header_type>`

Controls whether the corresponding forwarding headers are appended to requests
sent to the next proxy.

If you want to remove existing forwarded headers first, see
:ref:`steal_forwarded_for <config_server_http_proxy_steal_forwarded_for>` in
``http_proxy``.

See the doc of supported escapers for detailed protocol info.

**default**: classic, which means *X-Forwarded-\** headers will be appended

enable_tls_server
-----------------

**optional**, **type**: bool

Controls whether TLS is enabled for all local sites.

Requests to local sites without valid TLS server configuration are dropped.

**default**: false

.. _configuration_server_http_expose_global_tls_server:

global_tls_server
-----------------

**optional**, **type**: :external+values:ref:`openssl server config <conf_value_openssl_server_config>`

Global TLS server configuration used when the matched local site does not set
its own TLS server configuration.

**default**: not set

client_hello_recv_timeout
-------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for receiving the complete TLS ClientHello message.

**default**: 1s

.. _config_server_http_expose_site_group:

site_group
----------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Name of the :ref:`site group <configuration_site_group>` that provides Host /
SNI matching and per-site upstream settings.

Inline ``hosts`` / ``sites`` on this server are rejected. Put those rules in
the site group's :ref:`static_sites <conf_site_group_static_sites>`.

If the referenced group does not exist, an empty group is used and no site
matches.

**default**: not set
