.. _configuration_collector_regulate:

regulate
========

Collector that rewrites or normalizes metrics without aggregating them.

The following common keys are supported:

* :ref:`next <conf_collector_common_next>`
* :ref:`exporter <conf_collector_common_exporter>`

prefix
------

**optional**, **type**: :external+values:ref:`metric name prefix <conf_value_metric_name_prefix>`

Prefix added to all metric names.

drop_tags
---------

**optional**, **type**: :external+values:ref:`metric tag name <conf_value_metric_tag_name>` | seq

Tags to remove from all metrics.
