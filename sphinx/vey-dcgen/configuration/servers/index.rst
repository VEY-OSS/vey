.. _configuration_server:

******
Server
******

.. versionadded:: 0.10.0

Each server definition is a map with two required keys:

* :ref:`name <conf_server_common_name>`, which sets the server name
* :ref:`type <conf_server_common_type>`, which selects the concrete server type
  and therefore the remaining valid keys

The supported server types are:

* :ref:`udp <configuration_server_udp>`, which listens on one UDP address
* :ref:`unix <configuration_server_unix>`, which listens on one Unix datagram
  socket path
* :ref:`dummy <configuration_server_dummy>`, which does not listen

At least one server is required at startup. ``server`` can be reloaded. A new
or changed server is started, a removed server is stopped, and an unchanged
server keeps running. Changing ``type`` for an existing name replaces that
server. Changing ``listen`` on the same type restarts the listener and keeps
its statistics.

The main runtime stays single-threaded. See :doc:`../runtime` for how UDP
sockets are placed on workers.

Example::

   server:
     - name: local
       type: udp
       listen: 127.0.0.1:2999
     - name: public
       type: udp
       listen: "[::]:2999"
     - name: local_unix
       type: unix
       listen: /run/vey-dcgen/dcgen.sock

Servers
=======

.. toctree::
   :maxdepth: 2

   udp
   unix
   dummy

Common Keys
===========

This section describes keys shared by server types.

.. _conf_server_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the server name. This name is the ``server`` tag on the server's metrics.

.. _conf_server_common_type:

type
----

**required**, **type**: str

Set the server type.

The supported values are:

* ``udp``
* ``unix``, which is rejected on non-Unix platforms
* ``dummy``
