.. _configuration_server_plain_tcp_port:

plain_tcp_port
==============

This server provides a plain TCP port that can be placed in front of another
server.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

server
------

**required**, **type**: str

Name of the next server to which accepted connections are forwarded.

The next server must be able to accept TCP connections.

proxy_protocol
--------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

PROXY Protocol version expected on incoming TCP connections.

If set, connections with no matched PROXY Protocol message will be dropped.

.. note:: The ``ingress_network_filter`` option on this server always applies to
   the real socket client address.

**default**: not set, which means PROXY protocol won't be used

.. versionadded:: 1.7.19

proxy_protocol_read_timeout
---------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for reading a complete PROXY Protocol message.

**default**: 5s

.. versionadded:: 1.7.19
