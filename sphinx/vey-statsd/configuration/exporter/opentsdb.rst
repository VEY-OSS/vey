.. _configuration_exporter_opentsdb:

opentsdb
========

Exporter that sends metrics to OpenTSDB using the JSON `PUT API`_.

.. _PUT API: https://opentsdb.net/docs/build/html/api_http/put.html

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`HTTP Export Runtime <configuration_exporter_runtime_http>` is used:

- default port 4242
- all config keys supported

emit_interval
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s

sync_timeout
------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Controls the ``sync`` and ``sync_timeout`` query parameters.

**default**: not set

max_data_points
---------------

**optional**, **type**: usize

Maximum number of data points sent in a single HTTP request.

**default**: 50
