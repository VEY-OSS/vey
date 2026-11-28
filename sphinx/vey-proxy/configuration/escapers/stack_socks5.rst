.. _configuration_escaper_stack_socks5:

stack_socks5
============

.. versionadded:: 1.15.3

This escaper reaches the target through an upstream SOCKS5 proxy. The TCP
connection to that proxy is opened by the ``next`` escaper. UDP datagrams to
the address returned by UDP ASSOCIATE are also sent through ``next``. This
escaper then speaks SOCKS5 on those connections.

DNS resolution, source binding, and socket options belong to ``next``. This
escaper has no ``resolver`` key.

The following interfaces are supported:

* tcp connect
* udp relay
* udp connect
* http(s) forward

The following egress path selection value is supported:

* :ref:`egress upstream <proto_egress_path_selection_egress_upstream>`

  If matched, the ``addr`` field overrides ``proxy_addr`` for this request.

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`udp_sock_speed_limit <conf_escaper_common_udp_sock_speed_limit>`
* :ref:`peer negotiation timeout <conf_escaper_common_peer_negotiation_timeout>`
* :ref:`peer_health_check <conf_escaper_common_peer_health_check>`
* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

next
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the escaper that opens the connection to the selected proxy address.

proxy_addr
----------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>` | seq

Set the upstream SOCKS5 proxy address. The default port is ``1080`` and may be
omitted.

If a *seq* is used, each element must be a :external+values:ref:`weighted upstream addr <conf_value_weighted_upstream_addr>`.

A domain name is resolved by ``next``, not by this escaper.

proxy_addr_pick_policy
----------------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select the next proxy address.

The key for ketama/rendezvous/jump hash is *<client-ip>[-<username>]-<upstream-host>*.

**default**: random

proxy_username
--------------

**optional**, **type**: :external+values:ref:`username <conf_value_username>`

Set the proxy username. The SOCKS5 username/password method is used by default.

**alias**: ``proxy_user``

proxy_password
--------------

**optional**, **type**: :external+values:ref:`password <conf_value_password>`

Set the proxy password. Required if username is present.

**alias**: ``proxy_passwd``

transmute_udp_peer_ip
---------------------

**optional**, **type**: map | bool

Rewrite the UDP peer IP returned by the remote proxy when needed.

For a map value, each key is the returned IP and each value is the real IP to use.
If the map is empty, the peer IP from the TCP connection is used.

For a boolean value, ``true`` behaves like an empty map and ``false`` disables this feature.

When the feature is disabled, an unspecified UDP peer IP returned by the proxy
is still rewritten to the TCP peer IP.

**default**: false

end_on_control_closed
---------------------

**optional**, **type**: bool

End the UDP ASSOCIATE session whenever the peer closes the control TCP connection.

By default the session will be ended if:

- Any error occurs on the TCP control connection
- The TCP control connection closes cleanly after at least one UDP packet has been received

**default**: false

Example
-------

.. code-block:: yaml

   - name: stacked-socks
     type: stack_socks5
     next: direct
     proxy_addr: socks.example.net
     proxy_user: service-user
     proxy_passwd: secret
     transmute_udp_peer_ip: true
