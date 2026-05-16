.. _configuration_server_udp_tproxy:

udp_tproxy
==========

.. versionadded:: 1.13.3

This server is a transparent UDP listener that forwards traffic to the original
destination address.

See :ref:`transparent proxy <protocol_setup_transparent_proxy>` for the
required host firewall and routing setup.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`user_group <conf_server_common_user_group>`

  The user group must use fact-based authentication.
  It is used only when either ``auth_by_client_ip`` or ``auth_by_server_ip`` is
  enabled.

* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`udp_sock_speed_limit <conf_server_common_udp_sock_speed_limit>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`udp_relay_packet_size <conf_server_common_udp_relay_packet_size>`
* :ref:`udp_relay_yield_count <conf_server_common_udp_relay_yield_count>`
* :ref:`udp_relay_batch_count <conf_server_common_udp_relay_batch_count>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
* :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
* :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

listen
------

**optional**, **type**: :external+values:ref:`udp listen <conf_value_udp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

udp_conn_track
--------------

**optional**, **type**: :external+values:ref:`udp conn track <conf_value_udp_conn_track>`

Set the UDP connection track config.

**default**: set with default values

udp_socket_buffer
-----------------

**optional**, **type**: :external+values:ref:`socket buffer config <conf_value_socket_buffer_config>`

Socket-buffer configuration for the UDP socket.

.. note:: The buffer size of the socket at escaper side will also be set.

**default**: not set

auth_by_client_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_server_ip

Enables fact-based user authentication using the client IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false

auth_by_server_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_client_ip

Enables fact-based user authentication using the server IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false
