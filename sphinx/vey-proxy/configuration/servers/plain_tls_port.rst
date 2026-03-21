.. _configuration_server_plain_tls_port:

plain_tls_port
==============

This server provides a plain TLS port that can be placed in front of another
server.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tls_server <conf_server_common_tls_server>`
* :ref:`tls ticketer <conf_server_common_tls_ticketer>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`

  This is required for this server.

listen
------

**required**, **type**: :ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

server
------

**required**, **type**: str

Name of the next server to which accepted connections are forwarded.

The next server must be able to accept TLS connections.

proxy_protocol
--------------

**optional**, **type**: :ref:`proxy protocol version <conf_value_proxy_protocol_version>`

PROXY Protocol version expected on incoming TCP connections.

If set, connections with no matched PROXY Protocol message will be dropped.

The TLS handshake with the client will happen after we receive the PROXY Protocol message.

.. note:: The ``ingress_network_filter`` option on this server always applies to
   the real socket client address.

**default**: not set, which means PROXY protocol won't be used

.. versionadded:: 1.7.19

proxy_protocol_read_timeout
---------------------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Timeout for reading a complete PROXY Protocol message.

**default**: 5s

.. versionadded:: 1.7.19
