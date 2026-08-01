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
* :ref:`server <conf_server_common_server>`

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Set the listening socket configuration for this server.
