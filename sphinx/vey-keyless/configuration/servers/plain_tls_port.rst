.. _configuration_server_plain_tls_port:

plain_tls_port
==============

This server exposes a TLS port backed by rustls. It terminates TLS and hands
the decrypted stream over to the next server.

**alias**: plain_tls

.. versionadded:: 0.6.0

The following common keys are supported:

* :ref:`proxy_protocol <conf_server_common_proxy_protocol>`
* :ref:`proxy_protocol_read_timeout <conf_server_common_proxy_protocol_read_timeout>`
* :ref:`server <conf_server_common_server>`

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Set the listening socket configuration for this server.

tls_server
----------

**required**, **type**: :external+values:ref:`rustls server config <conf_value_rustls_server_config>`, **alias**: tls

Set the TLS parameters used to terminate the client connections.

tls_ticketer
------------

**optional**, **type**: :external+values:ref:`tls ticketer <conf_value_tls_ticketer>`

Set a local or remote rolling TLS ticket key provider.

**default**: not set
