.. _configuration_collector_aggregate:

aggregate
=========

Collector that aggregates incoming metrics.

The following common keys are supported:

* :ref:`next <conf_collector_common_next>`
* :ref:`exporter <conf_collector_common_exporter>`

emit_interval
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for aggregated metrics.

**default**: 1s

join_tags
---------

**optional**, **type**: :external+values:ref:`metric tag name <conf_value_metric_tag_name>` | seq

Tags used when joining metrics during aggregation.
