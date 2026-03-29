.. _configuration_server_socks_proxy:

socks_proxy
===========

This server provides a SOCKS proxy with support for TCP ``CONNECT`` and UDP
``ASSOCIATE``.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`
* :ref:`user_group <conf_server_common_user_group>`
* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tcp_sock_speed_limit <conf_server_common_tcp_sock_speed_limit>`
* :ref:`udp_sock_speed_limit <conf_server_common_udp_sock_speed_limit>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`dst_host_filter_set <conf_server_common_dst_host_filter_set>`
* :ref:`dst_port_filter <conf_server_common_dst_port_filter>`
* :ref:`tcp_copy_buffer_size <conf_server_common_tcp_copy_buffer_size>`
* :ref:`tcp_copy_yield_size <conf_server_common_tcp_copy_yield_size>`
* :ref:`udp_relay_packet_size <conf_server_common_udp_relay_packet_size>`
* :ref:`udp_relay_yield_size <conf_server_common_udp_relay_yield_size>`
* :ref:`udp_relay_batch_size <conf_server_common_udp_relay_batch_size>`
* :ref:`tcp_misc_opts <conf_server_common_tcp_misc_opts>`
* :ref:`udp_misc_opts <conf_server_common_udp_misc_opts>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
* :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
* :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

The authentication methods supported by this server depend on the type of the
configured user group.

+-------------+---------------------------+-------------------+
|auth scheme  |user group type            |is supported       |
+=============+===========================+===================+
|user         |hashed_user                |yes                |
+-------------+---------------------------+-------------------+
|gssapi       |gss_api                    |not yet            |
+-------------+---------------------------+-------------------+

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

.. versionadded:: 1.7.20 change listen config to be optional

use_udp_associate
-----------------

**optional**, **type**: bool, **alias**: enable_udp_associate, ``udp_associate_enabled``

Controls whether UDP ASSOCIATE is used instead of UDP CONNECT.

**default**: false

username_params
---------------

**optional**, **type**: :ref:`username_params <config_auth_username_params>`

Allows the egress context to be populated from username parameters.

This is mainly useful when the selected escaper chain includes
:ref:`comply_context <configuration_escaper_comply_context>`.

**default**: not set

.. versionadded:: 1.13.0

negotiation_timeout
-------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Maximum time allowed for SOCKS negotiation before processing the actual SOCKS
command.

**default**: 4s

udp_client_initial_timeout
--------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Maximum time to wait for the first UDP packet after the UDP port information is
returned to the client.

**default**: 30s

udp_bind_ipv4
-------------

**optional**, **type**: :external+values:ref:`list <conf_value_list>` of :external+values:ref:`ipv4 addr str <conf_value_ipv4_addr_str>`

IPv4 addresses used when binding the local UDP socket exposed to the SOCKS
client.
If not set, the server IP used for the TCP connection is used when creating the
UDP listener.

If set, the tcp connect can be in ipv6 address family.

Multiple addresses are allowed. One address is picked when the UDP listener is
created.

**default**: not set

udp_bind_ipv6
-------------

**optional**, **type**: :external+values:ref:`list <conf_value_list>` of :external+values:ref:`ipv6 addr str <conf_value_ipv6_addr_str>`

IPv6 addresses used when binding the local UDP socket exposed to the SOCKS
client.
If not set, the server IP used for the TCP connection is used when creating the
UDP listener.

If set, the tcp connect can be in ipv4 address family.

Multiple addresses are allowed. One address is picked when the UDP listener is
created.

**default**: not set

udp_bind_port_range
-------------------

**optional**, **type**: :external+values:ref:`port range <conf_value_port_range>`

UDP port range used when binding the local UDP socket exposed to the SOCKS
client.
If not set, the port is chosen by the operating system.

udp_socket_buffer
-----------------

**optional**, **type**: :external+values:ref:`socket buffer config <conf_value_socket_buffer_config>`

Socket-buffer configuration for the UDP socket.

.. note:: The buffer size of the socket at escaper side will also be set.

**default**: not set

transmute_udp_echo_ip
---------------------

**optional**, **type**: map | bool

Use this when the server should return an IP address other than the real local
bind IP of the UDP listener.

In map form, the key is the local IP and the value is the IP address the client
should use instead.
If no matching key is found, the unspecified address of the same family is
used.

If set to ``true``, an empty map is used.
If set to ``false``, the feature is disabled.

**default**: not set

.. versionchanged:: 1.9.9 allow bool value and change to use unspecified ip if no match records

auto_reply_local_ip_map
-----------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use transmute_udp_echo_ip instead

Example
-------

.. code-block:: yaml

   use_udp_associate: true
   udp_bind_ipv4:
     - 192.0.2.10
     - 192.0.2.11
   udp_bind_port_range: 40000-45000
   transmute_udp_echo_ip:
     192.0.2.10: 198.51.100.10
     192.0.2.11: 198.51.100.11
