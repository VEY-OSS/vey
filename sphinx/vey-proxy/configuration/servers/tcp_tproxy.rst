.. _configuration_server_tcp_tproxy:

tcp_tproxy
==========

.. versionadded:: 1.7.34

This server is a transparent TCP listener that forwards traffic to the original
destination address.

See :ref:`transparent proxy <protocol_setup_transparent_proxy>` for the
required host firewall and routing setup.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
* :ref:`user_group <conf_server_common_user_group>`

  The user group must use fact-based authentication.
  It is used only when either ``auth_by_client_ip`` or ``auth_by_server_ip`` is
  enabled.

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

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

On Linux, the listener is always switched into transparent mode. There is no
separate ``listen_transparent`` key for this server type.

auth_by_client_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_server_ip

Enables fact-based user authentication using the client IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false

.. versionadded:: 1.13.0

auth_by_server_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_client_ip

Enables fact-based user authentication using the server IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false

.. versionadded:: 1.13.0
