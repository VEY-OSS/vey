.. _configuration_server_sni_proxy:

sni_proxy
=========

This server is a lightweight TCP forward proxy that routes requests by looking
at TLS SNI or the HTTP ``Host`` header.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
* :ref:`user_group <conf_server_common_user_group>`

  The user group must use fact-based authentication.
  It is used only when either ``auth_by_client_ip`` or
  ``auth_by_server_name`` is enabled.

  .. versionadded:: 1.13.0

* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
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

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

.. versionadded:: 1.7.20 change listen config to be optional

listen_transparent
------------------

**optional**, **type**: bool

Set to ``true`` to enable transparent mode on the listening socket.

This flag is only available on Linux. When enabled, the listener socket is
placed into transparent mode before accept.

**default**: false

.. versionadded:: 1.13.0

auth_by_client_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_server_name

Enables fact-based user authentication using the client IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false

.. versionadded:: 1.13.0

auth_by_server_name
-------------------

**optional**, **type**: bool, **conflict**: auth_by_client_ip

Enables fact-based user authentication using the server name as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false

.. versionadded:: 1.13.0

tls_max_client_hello_size
-------------------------

**optional**, **type**: u32

Maximum size of the TLS ClientHello message.

**default**: 1 << 16

.. versionadded:: 1.9.9

request_wait_timeout
--------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout while waiting for the initial client data.

**default**: 60s

request_recv_timeout
--------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for receiving the complete initial request after the first bytes arrive.
The initial request may be either a TLS ClientHello message or an HTTP request.

**default**: 4s

protocol_inspection
-------------------

**optional**, **type**: :external+values:ref:`protocol inspection <conf_value_dpi_protocol_inspection>`

Basic protocol-inspection configuration.

**default**: set with default value

server_tcp_portmap
------------------

**optional**, **type**: :external+values:ref:`server tcp portmap <conf_value_dpi_server_tcp_portmap>`

Port mapping used for protocol inspection based on the server-side TCP port.

**default**: set with default value

client_tcp_portmap
------------------

**optional**, **type**: :external+values:ref:`client tcp portmap <conf_value_dpi_client_tcp_portmap>`

Port mapping used for protocol inspection based on the client-side TCP port.

**default**: set with default value

allowed_hosts
-------------

**optional**, **type**: :external+values:ref:`host matched object <conf_value_host_matched_object>` <:ref:`host <configuration_server_sni_proxy_host>`>

Host-matching rules that define which hosts this server should handle.

If not set, all requests will be handled.

**alias**: ``allowed_sites``

Example:

.. code-block:: yaml

  hosts:
    - exact_match:
        - www.example.net
        - example.net
      redirect_host: www.example.net:443 # all redirect to www.example.net:*
    - child_match: example.org # pass all *.example.org:*
    - exact_match: legacy.example.com
      redirect_port: 8443 # keep the original host, override only the port

**default**: not set

.. _configuration_server_sni_proxy_host:

Host
^^^^

Configuration for a matched SNI host.

redirect_host
"""""""""""""

**optional**, **type**: :external+values:ref:`host <conf_value_host>`

Overrides the host part of the upstream address.

If ``redirect_port`` is not set, the original destination port is kept.

**default**: not set

redirect_port
"""""""""""""

**optional**, **type**: u16

Overrides the port part of the upstream address.

If ``redirect_host`` is not set, the original destination host is kept.

**default**: not set
