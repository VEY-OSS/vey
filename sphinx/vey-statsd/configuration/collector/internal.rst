.. _configuration_collector_internal:

internal
========

Collector that emits internal runtime metrics.

The following common keys are supported:

* :ref:`next <conf_collector_common_next>`
* :ref:`exporter <conf_collector_common_exporter>`

emit_interval
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for internal metrics.

**default**: 1s
