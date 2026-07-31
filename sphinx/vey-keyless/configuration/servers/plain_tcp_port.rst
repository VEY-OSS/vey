.. _configuration_server_plain_tcp_port:

plain_tcp_port
==============

This server exposes a plain TCP port and hands every accepted connection over
to the next server.

It is useful when TLS is already terminated by a front load balancer, or when
the PROXY Protocol header has to be stripped before the key operation server
sees the stream.

**alias**: plain_tcp

.. versionadded:: 0.6.0

The following common keys are supported:

* :ref:`proxy_protocol <conf_server_common_proxy_protocol>`
* :ref:`proxy_protocol_read_timeout <conf_server_common_proxy_protocol_read_timeout>`

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Set the listening socket configuration for this server.

server
------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the name of the next server that will receive the accepted connections.

The next server must be able to accept plain TCP connections, which means it
must be a :ref:`keyless_cf <configuration_server_keyless_cf>` server without
its own ``tls_server`` config, or another port server.
