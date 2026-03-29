.. _configuration_server_plain_quic_port:

plain_quic_port
===============

.. versionadded:: 1.7.30

This server exposes a QUIC listening port in front of another local server.

It terminates QUIC locally and forwards accepted streams to the configured next
server.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tls ticketer <conf_server_common_tls_ticketer>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`

listen
------

**required**, **type**: :external+values:ref:`udp listen <conf_value_udp_listen>`

UDP listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

quic_server
-----------

**required**, **type**: :external+values:ref:`rustls server config <conf_value_rustls_server_config>`

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

Example
-------

.. code-block:: yaml

   listen: 0.0.0.0:443
   quic_server:
     cert_pairs:
       - certificate: server.crt
         private_key: server.key
   server: tcp-stream-in
