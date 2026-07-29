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

DragonFly BSD
=============

On DragonFly BSD, use the `ipfw fwd`_ rule. It does not rewrite the packet, and
for packets forwarded to a local address the kernel sets the local address of
the socket to the original destination, which is what the intercepting servers
read. No transparent socket option is involved, so a listener bound to a local
address is enough.

``bind_foreign`` on the ``direct_fixed`` escaper is not available here, as
DragonFly BSD has no ``IP_BINDANY`` equivalent to bind a non-local address.

.. _ipfw fwd: https://man.dragonflybsd.org/?command=ipfw&section=8

OpenBSD
=======

On OpenBSD, use the pf `divert-to`_ rule.

Transparent listeners and foreign binds require ``SO_BINDANY``. Use the
``rtable`` listen / misc option to set ``SO_RTABLE``.

.. _divert-to: https://man.openbsd.org/pf.conf.5#divert-to

NetBSD
======

On NetBSD, only the outbound half of transparent proxying is supported: the
``direct_fixed`` escaper can bind the client address with ``bind_foreign``,
using ``IP_BINDANY`` / ``IPV6_BINDANY`` to bind a non-local address (needs the
``KAUTH_REQ_NETWORK_BIND_ANYADDR`` privilege).

``foreign_port_hint_prefix`` is not supported on NetBSD (there is no
``SO_MARK`` / ``SO_USER_COOKIE`` equivalent used for this purpose).

The intercepting servers (``tcp_tproxy``, ``udp_tproxy`` and the
``listen_transparent`` option of ``sni_proxy``) are **not** available on
NetBSD. They recover the original destination from the accepted socket, which
requires the firewall to deliver the packet without rewriting it. NetBSD has
neither TPROXY nor divert sockets, and NPF `map`_ is address translation: the
destination is rewritten before the socket layer, so the server would see its
own listening address as the upstream.

Supporting interception on NetBSD requires querying the translation table for
the original destination, either ``IOC_NPF_CONN_LOOKUP`` (``npf_nat_lookup()``
in libnpf) for NPF, or ``SIOCGNATL`` for IPFilter. Neither is implemented yet.

.. _map: https://man.netbsd.org/npf.conf.5
