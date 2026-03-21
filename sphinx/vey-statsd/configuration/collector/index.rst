.. _configuration_collector:

*********
Collector
*********

The type for each collector config is *map*, with two always required keys:

* :ref:`name <conf_collector_common_name>`, which defines the collector name
* :ref:`type <conf_collector_common_type>`, which selects the concrete
  collector type and therefore determines how the remaining keys are
  interpreted

The available collector types are documented below.

Collectors
==========

.. toctree::
   :maxdepth: 1

   aggregate
   discard
   internal
   regulate

Common Keys
===========

This section describes common keys shared by many collector types.

.. _conf_collector_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Collector name.

.. _conf_collector_common_type:

type
----

**required**, **type**: str

Collector type.

.. _conf_collector_common_next:

next
----

**type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Next collector in the processing chain.

If the referenced collector does not exist, a default discard collector is used.

.. _conf_collector_common_exporter:

exporter
--------

**type**: :external+values:ref:`metric node name <conf_value_metric_node_name>` | seq

Exporter or exporters used by this collector.

If a referenced exporter does not exist, a default discard exporter is used.
