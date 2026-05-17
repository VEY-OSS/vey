.. _configuration_server_udp_stream:

udp_stream
==========

.. versionadded:: 1.13.3

This server forwards a local UDP listening port to one or more remote TCP upstreams.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`user_group <conf_server_common_user_group>`

  The user group must use fact-based authentication.
  It is used only when ``auth_by_client_ip`` is enabled.

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

**default**: set with default values, the max sessions count is 4096, which should be adjusted according the listen instance count

udp_socket_buffer
-----------------

**optional**, **type**: :external+values:ref:`socket buffer config <conf_value_socket_buffer_config>`

Socket-buffer configuration for the UDP socket.

.. note:: The buffer size of the socket at escaper side will also be set.

**default**: not set

upstream
--------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>` | seq

Remote address or addresses and port. The port is always required.

For *seq* value, each of its element must be :external+values:ref:`weighted upstream addr <conf_value_weighted_upstream_addr>`.

**alias**: proxy_pass

upstream_pick_policy
----------------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Policy used to select the upstream address.

The key for ketama/rendezvous/jump hash is *<client-ip><server-ip>*.

**default**: random

auth_by_client_ip
-----------------

**optional**, **type**: bool, **conflict**: auth_by_server_ip

Enables fact-based user authentication using the client IP address as the
authentication fact.

If enabled, ``user_group`` must also be set.

**default**: false
