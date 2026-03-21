.. _configuration_exporter_influxdb_v3:

influxdb_v3
===========

Exporter that sends metrics to InfluxDB v3 using the `v3 write_lp API`_.

.. _v3 write_lp API: https://docs.influxdata.com/influxdb3/enterprise/write-data/api-client-libraries/

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`HTTP Export Runtime <configuration_exporter_runtime_http>` is used:

- default port 8181
- all config keys supported

emit_interval
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s

database
--------

**required**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Database name.

token
-----

**optional**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Authentication token.

If not set, the value in environment variable `INFLUXDB3_AUTH_TOKEN` will be used.

**default**: not set

precision
---------

**optional**, **type**: string

Precision query parameter.

Allowed values are:

- second
- millisecond
- microsecond
- nanosecond

**default**: second

no_sync
-------

**optional**, **type**: bool

Controls the ``no_sync`` query parameter.

**default**: false

max_body_lines
--------------

**optional**, **type**: usize

Maximum number of line-protocol records sent in a single request.

**default**: 10000
