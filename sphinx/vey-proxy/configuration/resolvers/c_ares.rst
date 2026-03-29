.. _configuration_resolver_c_ares:

c_ares
======

Resolver backed by the ``c-ares`` DNS library.

The wrapper still owns cache behavior and protective timeouts. The remaining
keys on this page configure the underlying ``c-ares`` client.

The following common keys are supported:

* :ref:`graceful_stop_wait <conf_resolver_common_graceful_stop_wait>`
* :ref:`protective_query_timeout <conf_resolver_common_protective_query_timeout>`
* :ref:`positive_min_ttl <conf_resolver_common_positive_min_ttl>`
* :ref:`positive_max_ttl <conf_resolver_common_positive_max_ttl>`
* :ref:`negative_min_ttl <conf_resolver_common_negative_min_ttl>`

server
------

**optional**, **type**: str | seq

Name servers to use instead of those configured in ``/etc/resolv.conf``.

If the value is a string, it may contain one or more
:external+values:ref:`sockaddr str <conf_value_sockaddr_str>` values separated by whitespace.

If the value is a sequence, each element must be a
:external+values:ref:`sockaddr str <conf_value_sockaddr_str>`.

The default port ``53`` is used when no port is specified.

Servers from different address families may be configured together.

If omitted, the resolver uses the system configuration from ``/etc/resolv.conf``.

each_timeout
------------

**optional**, **type**: int, **unit**: ms

The number of milliseconds each name server is given to respond to a query on the first try.
After the first try, the timeout algorithm becomes more complicated, but scales linearly with the value of timeout.

**default**: 2000

.. versionchanged:: 1.7.27 change default value from 5000 to 2000 to match default values set in c-ares 1.20.1

each_tries
----------

**optional**, **type**: int

Number of attempts made to contact each name server before giving up.

**default**: 3

.. versionchanged:: 1.7.27 change default value from 2 to 3 to match default values set in c-ares 1.20.1

max_timeout
-----------

**optional**, **type**: int, **unit**: ms

Upper bound on the timeout between sequential retry attempts. Retry timeouts
increase from the base timeout value, and this setting caps the result.

**notes**: This will only have effect if link or build with c-ares 1.22.

**default**: 0, which is not explicitly set

.. versionadded:: 1.7.35

udp_max_quires
--------------

**optional**, **type**: int

Maximum number of UDP queries sent from a single ephemeral port to one DNS
server before a new ephemeral port is allocated.

**notes**: This will only have effect if link or build with c-ares 1.20.

**default**: 0, which is unlimited

.. versionadded:: 1.7.35

round_robin
-----------

**optional**, **type**: bool

If set to ``true``, nameservers are selected in round-robin order for each
resolution.

**default**: false

socket_send_buffer_size
-----------------------

**optional**, **type**: u32

Socket send-buffer size.

**default**: not set, which should be the value of /proc/sys/net/core/wmem_default

socket_recv_buffer_size
-----------------------

**optional**, **type**: u32

Socket receive-buffer size.

**default**: not set, which should be the value of /proc/sys/net/core/rmem_default

bind_ipv4
---------

**optional**, **type**: :external+values:ref:`ipv4 addr str <conf_value_ipv4_addr_str>`

IPv4 bind IP used when creating sockets for the resolver.

bind_ipv6
---------

**optional**, **type**: :external+values:ref:`ipv6 addr str <conf_value_ipv6_addr_str>`

IPv6 bind IP used when creating sockets for the resolver.

Example
-------

.. code-block:: yaml

   server:
     - 8.8.8.8
     - 1.1.1.1:53
   each_timeout: 2000
   each_tries: 3
   round_robin: true
