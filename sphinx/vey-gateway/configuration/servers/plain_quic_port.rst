.. _configuration_server_plain_quic_port:

plain_quic_port
===============

This server exposes a plain QUIC port that can be chained in front of other
servers.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`tls_ticketer <conf_server_common_tls_ticketer>`

listen
------

**required**, **type**: :external+values:ref:`udp listen <conf_value_udp_listen>`

Set the UDP listening socket configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

tls_server
----------

**required**, **type**: :external+values:ref:`rustls server config <conf_value_rustls_server_config>`

Set the TLS cryptographic configuration for this QUIC server.

quic_endpoint
-------------

**optional**, **type**: :external+values:ref:`quinn endpoint <conf_value_quinn_endpoint>`

Set the quinn endpoint config.

.. versionadded:: 0.4.0

server
------

**required**, **type**: str

Set the name of the next server that will receive accepted connections.

The next server should be able to accept tcp connections.
