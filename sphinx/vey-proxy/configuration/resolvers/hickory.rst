.. _configuration_resolver_hickory:

hickory
=======

Resolver implementation based on the ``hickory`` DNS library.

The following common keys are supported:

* :ref:`graceful_stop_wait <conf_resolver_common_graceful_stop_wait>`
* :ref:`protective_query_timeout <conf_resolver_common_protective_query_timeout>`
* :ref:`positive_min_ttl <conf_resolver_common_positive_min_ttl>`
* :ref:`positive_max_ttl <conf_resolver_common_positive_max_ttl>`
* :ref:`negative_min_ttl <conf_resolver_common_negative_min_ttl>`

server
------

**required**, **type**: str | seq

Configured name servers. All configured servers may be tried before a positive
response is returned.

If the value is a string, it may contain one or more
:external+values:ref:`ip addr str <conf_value_ip_addr_str>` values separated by whitespace.

If the value is a sequence, each element must be an
:external+values:ref:`ip addr str <conf_value_ip_addr_str>`.

server_port
-----------

**optional**, **type**: u16

Port to use when the default port is not suitable.

**default**: 53 for udp and tcp, 853 for dns-over-tls, 443 for dns-over-https

encryption
----------

**optional**, **type**: :external+values:ref:`dns encryption config <conf_value_dns_encryption_config>`

DNS encryption configuration.

**default**: not set

connect_timeout
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout used when establishing a TCP/TLS/QUIC connection to the target server.

**default**: 10s

.. versionadded:: 1.7.37

request_timeout
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout used while waiting for a response after a request has been sent to the
target server.

**default**: 10s

.. versionadded:: 1.7.37

each_tries
----------

**optional**, **type**: i32

Number of attempts made against one specific target server if no valid response
is received on the previous attempt.

.. note:: negative response is also considered valid

**default**: 2

.. versionchanged:: 1.7.37 this only control retries to a specific target server

each_timeout
------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Specify the timeout for waiting all responses from one specific target server.

**default**: 5s

retry_interval
--------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Retry interval between different target servers.

Responses may still arrive from previously tried servers, and the first
positive one is used.

**default**: 1s

.. versionadded:: 1.7.37

bind_ip
-------

**optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

Bind IP used when creating sockets for the resolver.

bind_interface
--------------

**optional**, **type**: :external+values:ref:`interface name <conf_value_interface_name>`

Binds the outgoing socket to a particular interface such as ``eth0``.

.. note:: This is only supported on Linux based OS.

**default**: not set

.. versionadded:: 1.11.3

tcp_misc_opts
-------------

**optional**, **type**: :external+values:ref:`tcp misc sock opts <conf_value_tcp_misc_sock_opts>`

Miscellaneous TCP socket options.

**default**: not set, nodelay is default enabled

.. versionadded:: 1.11.3

udp_misc_opts
-------------

**optional**, **type**: :external+values:ref:`udp misc sock opts <conf_value_udp_misc_sock_opts>`

Miscellaneous UDP socket options.

**default**: not set

.. versionadded:: 1.11.3
