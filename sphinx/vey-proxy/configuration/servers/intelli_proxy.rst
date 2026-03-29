.. _configuration_server_intelli_proxy:

intelli_proxy
=============

This server is a protocol-detecting entry port. It inspects the first bytes of
each connection and forwards the connection to another server accordingly.

The loader requires both ``http_server`` and ``socks_server``. Connections that
cannot be classified before ``protocol_detection_timeout`` expires are closed.

The following common keys are supported:

* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

http_server
-----------

**required**, **type**: str

Name of the next ``http_proxy`` server to which accepted connections are
forwarded.

socks_server
------------

**required**, **type**: str

Name of the next ``socks_proxy`` server to which accepted connections are
forwarded.

protocol_detection_timeout
--------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for protocol detection on each connection.

If timeout, the connection will be closed silently.

**default**: 4s

The legacy keys ``protocol_detection_channel_size`` and
``protocol_detection_max_jobs`` are accepted by the loader but ignored by the
current config implementation.

proxy_protocol
--------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

PROXY Protocol version expected on incoming TCP connections.

If set, connections with no matched PROXY Protocol message will be dropped.

.. note:: The ``ingress_network_filter`` option on this server always applies to
   the real socket client address.

**default**: not set, which means PROXY protocol won't be used

.. versionadded:: 1.7.28

proxy_protocol_read_timeout
---------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for reading a complete PROXY Protocol message.

**default**: 5s

.. versionadded:: 1.7.28

Example
-------

.. code-block:: yaml

   listen: 0.0.0.0:1080
   http_server: http-in
   socks_server: socks-in
   protocol_detection_timeout: 2s
