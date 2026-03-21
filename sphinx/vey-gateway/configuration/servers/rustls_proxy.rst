.. _configuration_server_rustls_proxy:

rustls_proxy
============

A layer-4 TLS reverse proxy server based on Rustls.

The following common keys are supported:

* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tcp_sock_speed_limit <conf_server_common_tcp_sock_speed_limit>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`tcp_copy_buffer_size <conf_server_common_tcp_copy_buffer_size>`
* :ref:`tcp_copy_yield_size <conf_server_common_tcp_copy_yield_size>`
* :ref:`tcp_misc_opts <conf_server_common_tcp_misc_opts>`
* :ref:`tls_ticketer <conf_server_common_tls_ticketer>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
* :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
* :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Set the listening socket configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

client_hello_recv_timeout
-------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the timeout for receiving the initial ClientHello bytes.

**default**: 10s

spawn_task_unconstrained
------------------------

**optional**, **type**: bool

Set whether task futures should be spawned with Tokio's unconstrained mode.

**default**: false

virtual_hosts
-------------

**required**, **type**: :external+values:ref:`host matched object <conf_value_host_matched_object>` <:ref:`host <configuration_server_rustls_proxy_host>`>

Set the virtual-host list, matched by SNI host rules.

If not set, all requests will be handled.

Example:

.. code-block:: yaml

  hosts:
    name: bench
    exact_match: bench.example.net
    cert_pairs:
      certificate: bench.example.net-ec256.crt
      private_key: bench.example.net-ec256.key
    backends:
      - http

**default**: not set

.. _configuration_server_rustls_proxy_host:

Host
^^^^

This section describes the configuration for a Rustls virtual host.

name
""""

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the virtual-host name.

**default**: not set

cert_pairs
""""""""""

**optional**, **type**: :external+values:ref:`tls cert pair <conf_value_tls_cert_pair>` or seq

Set certificate and private-key pairs for the TLS endpoint.

If not set, TLS protocol will be disabled.

**default**: not set

enable_client_auth
""""""""""""""""""

**optional**, **type**: bool

Set whether to require client authentication.

**default**: disabled

no_session_ticket
"""""""""""""""""

**optional**, **type**: bool

Disable TLS session tickets for stateless session resumption.

**default**: false

.. versionadded:: 0.3.3

no_session_cache
""""""""""""""""

**optional**, **type**: bool

Disable the TLS session cache for stateful session resumption.

**default**: false

.. versionadded:: 0.3.3

ca_certificate
""""""""""""""

**optional**, **type**: :external+values:ref:`tls certificates <conf_value_tls_certificates>`

Set the CA certificates used for client authentication. If not set, the
system default CA bundle is used.

**default**: not set

accept_timeout
""""""""""""""

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the timeout for completing the full TLS handshake.

**default**: 10s

request_rate_limit
""""""""""""""""""

**optional**, **type**: :external+values:ref:`rate limit quota <conf_value_rate_limit_quota>`

Set the request rate limit.

**default**: no limit

request_max_alive
"""""""""""""""""

**optional**, **type**: usize, **alias**: request_alive_max

Set the maximum number of concurrent live requests at the virtual-host level.

If not set, the effective limit is unbounded up to ``usize::MAX``.

**default**: no limit

tcp_sock_speed_limit
""""""""""""""""""""

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Set the speed limit for each TCP socket.

This will overwrite the server level :ref:`tcp_sock_speed_limit <conf_server_common_tcp_sock_speed_limit>`.

**default**: not set

task_idle_max_count
"""""""""""""""""""

**optional**, **type**: usize

Close the task after this many consecutive idle-check results return
``IDLE``.

This will overwrite the server level :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`.

**default**: not set

.. _conf_server_rustls_proxy_host_backend:

backends
""""""""

**required**, **type**: :external+values:ref:`alpn matched object <conf_value_alpn_matched_object>` <:ref:`backend <configuration_server_rustls_proxy_backend>`>

Set the backend list, matched by ALPN rules.

Example:

- A single ALPN value:

  .. code-block:: yaml

    backends:
      protocol: HTTP/1.1
      backend: foo

- Two single ALPN values:

  .. code-block:: yaml

    backends:
      - protocol: HTTP/1.1
        backend: foo
      - protocol: H2
        backend: bar

- No ALPN value:

  .. code-block:: yaml

    backends:
      - foo

**default**: not set

.. _configuration_server_rustls_proxy_backend:

Backend
^^^^^^^

This is the backend entry used in
:ref:`host backends <conf_server_rustls_proxy_host_backend>`.

It may be a map with the following key:

backend
"""""""

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the backend name to use.

It can also be written as a :external+values:ref:`metric node name <conf_value_metric_node_name>` value when needed.
