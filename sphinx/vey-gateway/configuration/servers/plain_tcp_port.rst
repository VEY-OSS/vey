.. _configuration_server_plain_tcp_port:

plain_tcp_port
==============

This server exposes a plain TCP port that can be chained in front of other
servers.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Set the listening socket configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

server
------

**required**, **type**: str

Set the name of the next server that will receive accepted connections.

The next server should be able to accept tcp connections.

proxy_protocol
--------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

Set the PROXY Protocol version expected on incoming TCP connections.

If this is set, connections without a valid PROXY Protocol header are dropped.

.. note:: The ``ingress_network_filter`` option on this server always matches
   the real socket peer address, not the address carried in the PROXY Protocol
   header.

**default**: not set, which means PROXY Protocol is disabled

proxy_protocol_read_timeout
---------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the timeout for receiving a complete PROXY Protocol header.

**default**: 5s
