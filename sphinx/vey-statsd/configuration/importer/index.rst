********
Importer
********

Each importer configuration item is a map with two required keys:

* :ref:`name <conf_importer_common_name>`, which defines the importer name
* :ref:`type <conf_importer_common_type>`, which selects the concrete importer
  type and therefore determines how the remaining keys are interpreted

The available importer types are documented below.

Importers
=========

.. toctree::
   :maxdepth: 1

   dummy
   statsd

Common Keys
===========

This section describes common keys shared by many importer types.

.. _conf_importer_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Importer name.

.. _conf_importer_common_type:

type
----

**required**, **type**: str

Importer type.

.. _conf_importer_common_collector:

collector
---------

**type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Collector used by this importer.

If the referenced collector does not exist, a default discard collector is used.

.. _conf_importer_common_listen_in_worker:

listen_in_worker
----------------

**optional**, **type**: bool

Controls whether the importer listens in each worker runtime when workers are enabled.

The listen instance count then matches the worker count.

**default**: false

.. _conf_importer_common_ingress_network_filter:

ingress_network_filter
----------------------

**optional**, **type**: :external+values:ref:`ingress network acl rule <conf_value_ingress_network_acl_rule>`

Ingress network filter for clients.

The client address used here is always the interpreted client address. That
means it is the raw socket peer address for direct listeners, or the address
provided by PROXY Protocol when applicable.

**default**: not set
