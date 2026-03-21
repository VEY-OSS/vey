.. _configuration_server_plain_quic_port:

plain_quic_port
===============

.. versionadded:: 1.7.30

This server provides a plain QUIC port that can be placed in front of another
server.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tls ticketer <conf_server_common_tls_ticketer>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`

listen
------

**required**, **type**: :ref:`udp listen <conf_value_udp_listen>`

UDP listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

quic_server
-----------

**required**, **type**: :ref:`rustls server config <conf_value_rustls_server_config>`

Cryptographic configuration for this QUIC server.

offline_rebind_port
-------------------

**optional**, **type**: u16

Rebind port used for graceful shutdown.

The new port should be reachable from the client or it won't work as expected.

**default**: not set

server
------

**required**, **type**: str

Name of the next server to which accepted connections are forwarded.

The next server must be able to accept TCP connections.
