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

.. _TPROXY: https://docs.kernel.org/networking/tproxy.html

FreeBSD
=======

On FreeBSD, the equivalent mechanism is the `ipfw`_ ``forward`` rule.

.. _ipfw: https://man.freebsd.org/cgi/man.cgi?query=ipfw

OpenBSD
=======

On OpenBSD, use the pf `divert-to`_ rule.

.. _divert-to: https://man.openbsd.org/pf.conf.5#divert-to
