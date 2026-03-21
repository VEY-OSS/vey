.. _configuration_discover:

********
Discover
********

Each discover definition is a map with two always-required keys:

* :ref:`name <conf_discover_common_name>`, which sets the discover name
* :ref:`type <conf_discover_common_type>`, which selects the concrete discover
  type and therefore the remaining valid keys

The supported discover types are documented below.

Discovers
=========

.. toctree::
   :maxdepth: 2

   static_addr
   host_resolver

Common Keys
===========

This section describes keys shared by multiple discover types.

.. _conf_discover_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the discover name.

.. _conf_discover_common_type:

type
----

**required**, **type**: str

Set the discover type.

.. _conf_discover_register_data:

Register Data
=============

Each discover type defines its own registration-data format. Follow the links
below for details.

+--------------+----------------------------------------------------------------------+
|Type          |Link                                                                  |
+==============+======================================================================+
|static_addr   |:ref:`static_addr data <conf_discover_static_addr_register_data>`     |
+--------------+----------------------------------------------------------------------+
|host_resolver |:ref:`host_resolver data <conf_discover_host_resolver_register_data>` |
+--------------+----------------------------------------------------------------------+
