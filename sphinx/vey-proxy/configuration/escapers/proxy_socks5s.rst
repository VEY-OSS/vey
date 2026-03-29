.. _configuration_escaper_proxy_socks5s:

proxy_socks5s
=============

.. versionadded:: 1.9.9

This escaper reaches the target through an upstream SOCKS5-over-TLS proxy.

The following interfaces are supported:

* tcp connect
* udp relay
* udp connect
* http(s) forward

The following egress path selection value is supported:

* :ref:`egress upstream <proto_egress_path_selection_egress_upstream>`

  If matched, the ``addr`` field overrides ``proxy_addr`` for this request.

  .. versionadded:: 1.13.0

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`resolver <conf_escaper_common_resolver>`, **required** only if *proxy_addr* is domain
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`
* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`udp_sock_speed_limit <conf_escaper_common_udp_sock_speed_limit>`
* :ref:`bind_interface <conf_escaper_common_bind_interface>`
* :ref:`no_ipv4 <conf_escaper_common_no_ipv4>`
* :ref:`no_ipv6 <conf_escaper_common_no_ipv6>`
* :ref:`tcp_connect <conf_escaper_common_tcp_connect>`
* :ref:`happy eyeballs <conf_escaper_common_happy_eyeballs>`
* :ref:`tcp_misc_opts <conf_escaper_common_tcp_misc_opts>`
* :ref:`udp_misc_opts <conf_escaper_common_udp_misc_opts>`
* :ref:`peer negotiation timeout <conf_escaper_common_peer_negotiation_timeout>`
* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

proxy_addr
----------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>` | seq

Set the target proxy address. The default port is ``1080`` and may be omitted.

If a *seq* is used, each element must be a :external+values:ref:`weighted upstream addr <conf_value_weighted_upstream_addr>`.

If any configured proxy address uses a domain name, ``resolver`` becomes
required.

proxy_addr_pick_policy
----------------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select the next proxy address.

The key for ketama/rendezvous/jump hash is *<client-ip>[-<username>]-<upstream-host>*.

**default**: random

tls_client
----------

**required**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Set TLS parameters for the local TLS client.
If set to an empty map, the default TLS client configuration is used.

**alias**: ``tls``

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

Set the TLS server name used to verify peer certificates.

If not set, the host part of each peer address is used.

**default**: not set

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

bind_ipv4
---------

**optional**, **type**: :external+values:ref:`ipv4 addr str <conf_value_ipv4_addr_str>`

Set the bind IP address for IPv4 sockets.

**default**: not set

bind_ipv6
---------

**optional**, **type**: :external+values:ref:`ipv6 addr str <conf_value_ipv6_addr_str>`

Set the bind IP address for IPv6 sockets.

**default**: not set

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Configure TCP keepalive.

User-level TCP keepalive settings are not applied.

**default**: 60s

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

.. versionadded:: 1.7.19

Example:

.. code-block:: yaml

   - name: corp-socks-tls
     type: proxy_socks5s
     proxy_addr: socks.example.net:1080
     resolver: local-dns
     tls: {}
     tls_name: socks.example.net
     proxy_user: service-user
     proxy_passwd: secret

end_on_control_closed
---------------------

**optional**, **type**: bool

End the UDP ASSOCIATE session whenever the peer closes the control TCP connection.

By default the session will be ended if:

- Any error occurs on the TCP control connection
- The TCP control connection closes cleanly after at least one UDP packet has been received

**default**: false

.. versionadded:: 1.9.9
