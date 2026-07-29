.. _protocol_setup_transparent_proxy:

=================
Transparent Proxy
=================

This section lists the operating-system features that can be used to deploy
``vey-proxy`` as a transparent proxy.

Linux
=====

On Linux, transparent proxying is typically implemented with netfilter
`TPROXY`_. Use it to redirect traffic to ``vey-proxy`` while preserving the
original destination address.

For GWLB-style egress that must preserve the client IP and port without
``EADDRINUSE``, see ``foreign_port_hint_prefix`` on the ``direct_fixed``
escaper: the original client port is encoded into ``SO_MARK`` for a tc/XDP
rewriter.

.. _TPROXY: https://docs.kernel.org/networking/tproxy.html

FreeBSD
=======

On FreeBSD, the equivalent mechanism is the `ipfw`_ ``forward`` rule.

Transparent listeners and foreign binds require ``IP_BINDANY`` /
``IPV6_BINDANY`` so the process can bind non-local addresses. Use the
``user_cookie`` listen / misc option to set ``SO_USER_COOKIE``.

``foreign_port_hint_prefix`` on ``direct_fixed`` encodes the client port into
``SO_USER_COOKIE`` the same way (visible to ipfw ``sockarg``).

.. _ipfw: https://man.freebsd.org/cgi/man.cgi?query=ipfw

OpenBSD
=======

On OpenBSD, use the pf `divert-to`_ rule.

Transparent listeners and foreign binds require ``SO_BINDANY``. Use the
``rtable`` listen / misc option to set ``SO_RTABLE``.

.. _divert-to: https://man.openbsd.org/pf.conf.5#divert-to

NetBSD
======

On NetBSD, use NPF `map`_ rules for address translation when building a
transparent proxy path. Mapping directions:

- ``<-`` inbound NAT (rewrite destination; used for redirect / port forward)
- ``->`` outbound NAT (rewrite source)
- ``<->`` bi-directional NAT

Transparent listeners and foreign binds require ``IP_BINDANY`` /
``IPV6_BINDANY`` so the process can bind non-local addresses (needs
``KAUTH_REQ_NETWORK_BIND_ANYADDR``).

``foreign_port_hint_prefix`` is not supported on NetBSD (there is no
``SO_MARK`` / ``SO_USER_COOKIE`` equivalent used for this purpose).

Example NPF inbound ``map`` (port-forward style; adjust for your topology)::

    # Redirect TCP $ext_if:1080 to the local transparent listener
    map $ext_if dynamic proto tcp $proxy_ip port 1080 <- $ext_if port 1080

Outbound source translation when leaving the proxy host can use ``->``::

    map $ext_if dynamic $proxy_net -> ifaddrs($ext_if)

.. _map: https://man.netbsd.org/npf.conf.5
