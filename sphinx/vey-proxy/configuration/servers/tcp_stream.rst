.. _configuration_server_tcp_stream:

tcp_stream
==========

This server forwards a local TCP listening port to one or more remote TCP
upstreams.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
* :ref:`user_group <conf_server_common_user_group>`

  The user group must use fact-based authentication.
  It is used only when ``auth_by_client_ip`` is enabled.

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

upstream
--------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>` | seq

Remote address or addresses and port. The port is always required.

For *seq* value, each of its element must be :external+values:ref:`weighted upstream addr <conf_value_weighted_upstream_addr>`.

**alias**: proxy_pass

Example:

.. code-block:: yaml

   upstream:
     - addr: db-a.internal.example:5432
       weight: 3
     - addr: db-b.internal.example:5432
       weight: 1

upstream_pick_policy
----------------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Policy used to select the upstream address.

The key for ketama/rendezvous/jump hash is *<client-ip><server-ip>*.

**default**: random

tls_client
----------

**optional**, **type**: bool | :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Controls whether a TLS handshake is performed with the upstream.

When set to ``true``, ``vey-proxy`` creates a default OpenSSL client
configuration with per-site session caching. When set to a map, the supplied
TLS client configuration is used.

**default**: disabled

upstream_tls_name
-----------------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

Explicit TLS server name used for upstream certificate verification.

If not set, the host of upstream address will be used.

When ``tls_client`` is enabled and this key is not set, the host from the first
configured upstream entry is used automatically.

**default**: not set

auth_by_client_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_server_ip

Enables fact-based user authentication using the client IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false

.. versionadded:: 1.13.0
