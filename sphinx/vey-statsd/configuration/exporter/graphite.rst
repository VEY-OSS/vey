.. _configuration_exporter_graphite:

graphite
========

Exporter that sends metrics to Graphite using the plaintext protocol.

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`Stream Export Runtime <configuration_exporter_runtime_stream>` is used:

- default port 2003
- all config keys supported

emit_interval
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s
