.. _configuration_server_http_rproxy:

http_rproxy
===========

This server provides an HTTP reverse proxy.

This server terminates the client-side HTTP session locally and then forwards
requests to configured upstream sites selected by the ``hosts`` match table.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
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

.. _config_server_http_rproxy_server_id:

server_id
---------

**optional**, **type**: :external+values:ref:`http server id <conf_value_http_server_id>`

Server ID. If set, the ``X-VEY-Remote-Connection-Info`` header is added to
responses, and the value is also used in the ``Via`` header added to requests.

**default**: not set

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

.. _config_server_http_rproxy_log_uri_max_chars:

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

http_forward_upstream_keepalive
-------------------------------

**optional**, **type**: :external+values:ref:`http keepalive <conf_value_http_keepalive>`

HTTP keepalive configuration at the server level.

**default**: set with default value

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

.. _configuration_server_http_rproxy_global_tls_server:

global_tls_server
-----------------

**optional**, **type**: :external+values:ref:`rustls server config <conf_value_rustls_server_config>`

Global TLS server configuration used when the matched local site does not set
its own TLS server configuration.

**default**: not set

client_hello_recv_timeout
-------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for receiving the complete TLS ClientHello message.

**default**: 1s

hosts
-----

**required**, **type**: :external+values:ref:`host matched object <conf_value_host_matched_object>` <:ref:`host <configuration_server_http_rproxy_host>`>, **alias**: sites

Host-matching rules that define which hosts this reverse proxy should handle.

Example 1:

.. code-block:: yaml

  hosts:
    services:
      upstream: www.example.net

Example 2:

.. code-block:: yaml

  hosts:
    - exact_match:
        - www.example.net
        - example.net
      services:
        upstream: www.example.net
    - child_match: example.org
      set_default: true
      services:
        upstream: www.example.org

**default**: not set

.. _configuration_server_http_rproxy_host:

Host
^^^^

Configuration for each local host handled by this server.

tls_server
""""""""""

**optional**, **type**: :external+values:ref:`rustls server config <conf_value_rustls_server_config>`

TLS server configuration for this local site.

If not set, the :ref:`global tls server <configuration_server_http_rproxy_global_tls_server>` config will be used.

**default**: not set

upstream
""""""""

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>`

Target upstream address. The default port is ``80`` and may be omitted.

tls_client
""""""""""

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

TLS parameters for the local client side when HTTPS to the upstream is needed.
If set to an empty map, the default configuration is used.

**default**: not set

tls_name
""""""""

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

TLS server name used to verify the upstream site's certificate.

If not set, the host part of the upstream address will be used.

**default**: not set

Example
"""""""

.. code-block:: yaml

   hosts:
     - exact_match:
         - www.example.net
         - example.net
       services:
         upstream: app.example.net:8080
     - child_match: example.org
       set_default: true
       services:
         upstream: app.example.org:8080
         tls_client: {}
