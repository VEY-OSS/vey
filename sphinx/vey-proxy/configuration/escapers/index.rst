.. _configuration_escaper:

*******
Escaper
*******

Each escaper configuration item is a map with two required keys:

* :ref:`name <conf_escaper_common_name>`, which defines the escaper name
* :ref:`type <conf_escaper_common_type>`, which selects the concrete escaper
  type and therefore determines how the remaining keys are interpreted

The available escaper types are documented below.

Escapers
========

.. toctree::
   :maxdepth: 1

   comply_audit
   comply_context
   dummy_deny
   direct_fixed
   direct_float
   divert_tcp
   proxy_float
   proxy_http
   proxy_https
   proxy_socks5
   proxy_socks5s
   route_mapping
   route_query
   route_resolved
   route_geoip
   route_select
   route_upstream
   route_client
   route_failover
   trick_float

Common Keys
===========

This section describes common keys shared by many escaper types.

.. _conf_escaper_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

The escaper name.

.. _conf_escaper_common_type:

type
----

**required**, **type**: str

The escaper type.

.. _conf_escaper_common_shared_logger:

shared_logger
-------------

**optional**, **type**: ascii

Makes this escaper use a logger that runs on a shared thread.

**default**: not set

.. _conf_escaper_common_resolver:

resolver
--------

**type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

The resolver used by this escaper.

If the referenced resolver does not exist in the configuration, a default
``DenyAll`` resolver is used.

.. _conf_escaper_common_resolve_strategy:

resolve_strategy
-----------------

**optional**, **type**: :external+values:ref:`resolve strategy <conf_value_resolve_strategy>`

DNS resolution strategy.

.. _conf_escaper_common_tcp_sock_speed_limit:

tcp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Per-TCP-socket speed limit.

**default**: no limit

tcp_conn_speed_limit
--------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use tcp_sock_speed_limit instead

tcp_conn_limit
--------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use tcp_sock_speed_limit instead

conn_limit
----------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use tcp_sock_speed_limit instead

.. _conf_escaper_common_udp_sock_speed_limit:

udp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`udp socket speed limit <conf_value_udp_sock_speed_limit>`

Per-UDP-socket speed limit.

**default**: no limit

udp_relay_speed_limit
---------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use udp_sock_speed_limit instead

udp_relay_limit
---------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use udp_sock_speed_limit instead

relay_limit
-----------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use udp_sock_speed_limit instead

.. _conf_escaper_common_bind_interface:

bind_interface
--------------

**optional**: **type**: :external+values:ref:`interface name <conf_value_interface_name>`

Binds the outgoing socket to a particular interface such as ``eth0``.

**default**: not set

.. versionadded:: 1.9.9

.. _conf_escaper_common_no_ipv4:

no_ipv4
-------

**optional**, **type**: bool

Disables IPv4. This setting should remain compatible with
:ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`.

**default**: false

.. _conf_escaper_common_no_ipv6:

no_ipv6
-------

**optional**, **type**: bool

Disables IPv6. This setting should remain compatible with
:ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`.

**default**: false

.. _conf_escaper_common_tcp_connect:

tcp_connect
-----------

**optional**, **type**: :external+values:ref:`tcp connect <conf_value_tcp_connect>`

TCP connect parameters.

.. note:: For *direct* escapers, user-level TCP connect parameters further limit
   the final effective values.

.. _conf_escaper_common_happy_eyeballs:

happy_eyeballs
--------------

**optional**, **type**: :external+values:ref:`happy eyeballs <conf_value_happy_eyeballs>`

Happy Eyeballs configuration.

**default**: default HappyEyeballs config

.. _conf_escaper_common_tcp_misc_opts:

tcp_misc_opts
-------------

**optional**, **type**: :external+values:ref:`tcp misc sock opts <conf_value_tcp_misc_sock_opts>`

Miscellaneous TCP socket options.

**default**: not set, nodelay is default enabled

.. _conf_escaper_common_udp_misc_opts:

udp_misc_opts
-------------

**optional**, **type**: :external+values:ref:`udp misc sock opts <conf_value_udp_misc_sock_opts>`

Miscellaneous UDP socket options.

**default**: not set

.. _conf_escaper_common_default_next:

default_next
------------

**required**, **type**: str

Default next-hop escaper for *route* escapers.

.. _conf_escaper_common_pass_proxy_userid:

pass_proxy_userid
-----------------

**optional**, **type**: bool

Controls whether the user ID (username) is forwarded to the next proxy.

If enabled, native Basic authentication is used when negotiating with the next
proxy. The username field contains the real username, and the password field is
set to the package name (``vey-proxy`` unless forked).

**default**: false

.. note:: This will conflict with the real auth of next proxy.

.. _conf_escaper_common_peer_negotiation_timeout:

peer_negotiation_timeout
------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the negotiation timeout for next proxy peers.

**default**: 10s

.. _conf_escaper_common_extra_metrics_tags:

extra_metrics_tags
------------------

**optional**, **type**: :external+values:ref:`static metrics tags <conf_value_static_metrics_tags>`

Set extra metrics tags that should be added to escaper stats and user stats already with escaper tags added.

**default**: not set
