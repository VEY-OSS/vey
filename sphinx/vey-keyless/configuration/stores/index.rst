.. _configuration_store:

*****
Store
*****

Set the key store.
This section defines private-key stores.

Each store configuration item is a map with two required keys:

* :ref:`name <conf_store_common_name>`, which defines the store name
* :ref:`type <conf_store_common_type>`, which selects the concrete store type
  and therefore determines how the remaining keys are interpreted

The available store types are documented below.

Stores
======

.. toctree::
   :maxdepth: 2

   local

Common Keys
===========

This section describes common keys shared by many store types.

.. _conf_store_common_name:

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Store name.

.. _conf_store_common_type:

**required**, **type**: str

Store type.
