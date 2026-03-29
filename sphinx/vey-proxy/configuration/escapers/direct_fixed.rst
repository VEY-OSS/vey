.. _configuration_escaper_direct_fixed:

direct_fixed
============

This escaper sends traffic to the target upstream directly from the local host.

The following interfaces are supported:

* tcp connect
* udp relay
* udp connect
* http(s) forward
* ftp over http

The following egress path selection value is supported:

* :ref:`number id <proto_egress_path_selection_number_id>`

  The selected node ID is used to choose the bind IP address from ``bind_ip``.

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`resolver <conf_escaper_common_resolver>`, **required**
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`

  The user custom resolve strategy will be taken into account.

* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`udp_sock_speed_limit <conf_escaper_common_udp_sock_speed_limit>`
* :ref:`bind_interface <conf_escaper_common_bind_interface>`
* :ref:`no_ipv4 <conf_escaper_common_no_ipv4>`
* :ref:`no_ipv6 <conf_escaper_common_no_ipv6>`
* :ref:`tcp_connect <conf_escaper_common_tcp_connect>`

  The user tcp connect params will be taken into account.

* :ref:`happy eyeballs <conf_escaper_common_happy_eyeballs>`
* :ref:`tcp_misc_opts <conf_escaper_common_tcp_misc_opts>`
* :ref:`udp_misc_opts <conf_escaper_common_udp_misc_opts>`
* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

bind_ip
-------

**optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>` | seq

Set the bind ip address(es) for sockets.

For *seq* value, each of its element must be :external+values:ref:`ip addr str <conf_value_ip_addr_str>`.
Only random selection is supported. Use a *route* escaper if you need more control.

IPv4 and IPv6 addresses are kept in separate internal lists. If you disable one
address family, entries from that family become unusable.

**default**: not set

bind_foreign
------------

**optional**, **type**: bool

Set to true to bind to the same foreign IP address as the client. This is useful with Linux TPROXY.

**default**: false

.. versionadded:: 1.13.0

bind_foreign_port
-----------------

**optional**, **type**: bool

Set to true if you also want to bind to the same foreign port when `bind_foreign` is also enabled.

.. note:: This may cause ``EADDRINUSE`` if two connections use the same client address.

.. versionadded:: 1.13.0

**default**: false

egress_network_filter
---------------------

**optional**, **type**: :external+values:ref:`egress network acl rule <conf_value_egress_network_acl_rule>`

Set the network filter for the resolved remote IP address.

**default**: all permitted except for loop-back and link-local addresses

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Configure TCP keepalive.

User-level TCP keepalive settings are also applied.

**default**: no keepalive set

resolve_redirection
-------------------

**optional**, **type**: :external+values:ref:`resolve redirection <conf_value_resolve_redirection>`

Set DNS redirection rules at the escaper level.

**default**: not set

enable_path_selection
---------------------

**optional**, **type**: bool

Enable path selection.

.. note:: Path selection must also be enabled on the server side, or this option has no effect.

When enabled, the :ref:`number id <proto_egress_path_selection_number_id>`
value is interpreted as the 1-based index of the chosen bind address within the
family-specific bind list.

**default**: false

Example:

.. code-block:: yaml

   - name: direct-egress
     type: direct_fixed
     resolver: local-dns
     bind_ip:
       - 192.0.2.10
       - 192.0.2.11
       - 2001:db8::10
     enable_path_selection: true

use_proxy_protocol
------------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

Set the PROXY protocol version to use for outgoing TCP connections, except FTP data connections.

**default**: not set, which means PROXY protocol won't be used

.. versionadded:: 1.11.3
