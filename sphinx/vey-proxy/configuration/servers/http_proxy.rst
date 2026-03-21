.. _configuration_server_http_proxy:

http_proxy
==========

This server implements an HTTP proxy, including both HTTP forward and HTTP
CONNECT support.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
* :ref:`user_group <conf_server_common_user_group>`
* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tls_server <conf_server_common_tls_server>`
* :ref:`tls ticketer <conf_server_common_tls_ticketer>`
* :ref:`tcp_sock_speed_limit <conf_server_common_tcp_sock_speed_limit>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`dst_host_filter_set <conf_server_common_dst_host_filter_set>`
* :ref:`dst_port_filter <conf_server_common_dst_port_filter>`
* :ref:`tcp_copy_buffer_size <conf_server_common_tcp_copy_buffer_size>`
* :ref:`tcp_copy_yield_size <conf_server_common_tcp_copy_yield_size>`
* :ref:`tcp_misc_opts <conf_server_common_tcp_misc_opts>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
* :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
* :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

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

local_server_name
-----------------

**optional**, **type**: :external+values:ref:`host <conf_value_host>` | seq

List of local server names.

A request is treated as a local request when:

- no local server name set

  The URL in the HTTP request header is relative

- local server name has been set

  The method is not ``CONNECT`` and the server name in the ``Host`` header
  matches one of the configured local server names

Set this if you want to enable support for Well-Known URIs.

.. versionadded:: 1.11.5

.. _config_server_http_proxy_server_id:

server_id
---------

**optional**, **type**: :external+values:ref:`http server id <conf_value_http_server_id>`

Server ID. If set, the ``X-BD-Remote-Connection-Info`` header is added to the
response.

**default**: not set

auth_realm
----------

**optional**, **type**: :external+values:ref:`ascii str <conf_value_ascii_str>`

Authentication realm.

**default**: proxy

username_params
---------------

**optional**, **type**: :ref:`username_params <config_auth_username_params>`

Allows the egress context to be populated from username parameters.

**default**: not set

.. versionadded:: 1.13.0

.. _conf_server_http_proxy_tls_client:

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

TLS client parameters used for HTTPS-forward requests.

**default**: set with default value

ftp_client
----------

**optional**, **type**: :external+values:ref:`ftp client config <conf_value_ftp_client_config>`

FTP client configuration used for FTP-over-HTTP requests.

**default**: set with default value

req_header_recv_timeout
-----------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Maximum time to wait for the full request header after the client connection
becomes readable.

**default**: 30s

.. _conf_server_http_proxy_rsp_header_recv_timeout:

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

.. _config_server_http_proxy_log_uri_max_chars:

log_uri_max_chars
-----------------

**optional**, **type**: usize

Maximum number of URI characters recorded in logs.

If the user-level configuration also sets this value, the user-level setting
takes precedence. See :ref:`user config option <config_user_log_uri_max_chars>`.

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

allow_custom_host
-----------------

**optional**, **type**: bool

Controls whether a custom ``Host`` header is allowed. If set to ``false``, the
``Host`` header must contain the same domain or IP address as the request line.

**default**: true

.. note:: ``vey-proxy`` does not require a ``Host`` header to be present, no
   matter how this option is set.

drop_default_port_in_host
-------------------------

**optional**, **type**: bool

Controls whether the default port should be removed from the ``Host`` header
before the request is sent upstream.

The default ports are:

  - HTTP 80
  - HTTPS 443

**default**: false

.. versionadded:: 1.11.10

body_line_max_length
--------------------

**optional**, **type**: int

Maximum line length for lines in the HTTP body, such as trailer fields and
chunk-size lines.

**default**: 8192

http_forward_upstream_keepalive
-------------------------------

**optional**, **type**: :external+values:ref:`http keepalive <conf_value_http_keepalive>`

HTTP keepalive configuration at the server level.

**default**: set with default value

.. _config_server_http_proxy_http_forward_mark_upstream:

http_forward_mark_upstream
--------------------------

**optional**, **type**: bool

If enabled, the ``X-BD-Upstream-Id`` header is added to responses received from
upstream, using the value of :ref:`server_id <config_server_http_proxy_server_id>`.
Responses generated locally do not contain this header.

**default**: false

.. _config_server_http_proxy_echo_chained_info:

echo_chained_info
-----------------

**optional**, **type**: bool

Controls whether custom response headers are added to expose chaining
information about the direct upstream connection.

The custom headers are:

- X-BD-Upstream-Addr
- X-BD-Outgoing-IP

**default**: false

untrusted_read_speed_limit
--------------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Enables untrusted reading of request bodies that do not yet have authentication
information, and sets the corresponding read-rate limit.

Use this if you need compatibility with buggy Java HTTP clients that do not
handle ``407`` responses promptly.

**default**: not set, which means untrusted read is disabled

untrusted_read_limit
--------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use untrusted_read_speed_limit instead

.. _config_server_http_proxy_egress_path_selection_header:

egress_path_selection_header
----------------------------

**optional**, **type**: str, **alias**: path_selection_header

HTTP header name used for egress path selection.

**default**: not set

.. _config_server_http_proxy_steal_forwarded_for:

steal_forwarded_for
-------------------

**optional**, **type**: bool

Controls whether the ``Forwarded`` and ``X-Forwarded-For`` headers are removed
from the client request.

.. note::

  To remove these headers from HTTPS traffic, TLS interception must be enabled
  and the corresponding option must also be set in the auditor's
  :ref:`h1 interception <conf_auditor_h1_interception>` configuration.

**default**: false
