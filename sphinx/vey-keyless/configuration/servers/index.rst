.. _configuration_server:

******
Server
******

Each server definition is a map with one always-required key:

* :ref:`name <conf_server_common_name>`, which sets the server name

and one optional key:

* :ref:`type <conf_server_common_type>`, which selects the concrete server type
  and therefore the remaining valid keys

Servers fall into two groups:

* *key operation servers*, such as :ref:`cloudflare <configuration_server_cloudflare>`,
  which speak a key operation protocol and run the private key operations
* *port servers*, such as :ref:`plain_tcp_port <configuration_server_plain_tcp_port>`
  and :ref:`plain_tls_port <configuration_server_plain_tls_port>`, which only own
  a listening socket and hand every accepted connection over to a next server

A port server is chained to its next server through the :ref:`server
<conf_server_common_server>` key. The reference is verified at config load time,
so a missing or self-referencing next server is rejected before the daemon
starts.

A key operation server may be used in either of two ways:

* with its own ``listen`` key, so it accepts client connections directly
* without a ``listen`` key, so it only serves the connections sent by port
  servers

The supported server types are documented below.

Servers
=======

.. toctree::
   :maxdepth: 2

   cloudflare
   plain_tcp_port
   plain_tls_port

Common Keys
===========

This section describes keys shared by multiple server types.

.. _conf_server_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the server name.

.. _conf_server_common_type:

type
----

**optional**, **type**: str

Set the server type.

**default**: cloudflare

.. versionadded:: 0.6.0

.. _conf_server_common_server:

server
------

**required for port servers**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the name of the next server that will receive the accepted connections.

.. versionadded:: 0.6.0

.. _conf_server_common_proxy_protocol:

proxy_protocol
--------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

Set the PROXY Protocol version expected on incoming TCP connections.

If this is set, connections without a valid PROXY Protocol header are dropped.

**default**: not set, which means PROXY Protocol is disabled

.. versionadded:: 0.6.0

.. _conf_server_common_proxy_protocol_read_timeout:

proxy_protocol_read_timeout
---------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the timeout for receiving a complete PROXY Protocol header.

**default**: 5s

.. versionadded:: 0.6.0

.. _conf_server_common_shared_logger:

shared_logger
-------------

**optional**, **type**: ascii

Makes this server use a logger running on a shared thread.

**default**: not set

.. _conf_server_common_extra_metrics_tags:

extra_metrics_tags
------------------

**optional**, **type**: :external+values:ref:`static metrics tags <conf_value_static_metrics_tags>`

Extra metric tags added to server statistics.

**default**: not set
