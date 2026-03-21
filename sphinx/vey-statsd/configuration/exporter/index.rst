********
Exporter
********

Each exporter configuration item is a map with two required keys:

* :ref:`name <conf_exporter_common_name>`, which defines the exporter name
* :ref:`type <conf_exporter_common_type>`, which selects the concrete exporter
  type and therefore determines how the remaining keys are interpreted

The available exporter types are documented below.

Exporters
=========

.. toctree::
   :maxdepth: 1

   console
   discard
   graphite
   influxdb_v2
   influxdb_v3
   memory
   opentsdb

Common Keys
===========

This section describes common keys shared by many exporter types.

.. _conf_exporter_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Exporter name.

.. _conf_exporter_common_type:

type
----

**required**, **type**: str

Exporter type.

.. _conf_exporter_common_prefix:

prefix
------

**optional**, **type**: :external+values:ref:`metric name prefix <conf_value_metric_name_prefix>`

Prefix added to all metric names.

.. _conf_exporter_common_global_tags:

global_tags
-----------

**optional**, **type**: :external+values:ref:`static metrics tags <conf_value_static_metrics_tags>`

Static tags added to all metrics.

Export Runtimes
===============

An export runtime is the loop that emits metrics at the configured
``emit_interval``.

.. _configuration_exporter_runtime_stream:

Stream Export Runtime
---------------------

host
^^^^

**required**, **type**: :external+values:ref:`host <conf_value_host>`

Peer host name.

port
^^^^

**required**, **type**: u16

Peer server port.

**default**: each exporter will set a default port value

resolve_retry_wait
^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Wait time before retrying after a resolution failure.

**default**: 30s

connect_retry_wait
^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Wait time before retrying after a connection failure.

**default**: 10s

.. _configuration_exporter_runtime_http:

HTTP Export Runtime
-------------------

host
^^^^

**required**, **type**: :external+values:ref:`host <conf_value_host>`

Peer host name.

port
^^^^

**required**, **type**: u16

Peer server port.

**default**: each exporter will set a default port value

resolve_retry_wait
^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Wait time before retrying after a resolution failure.

**default**: 30s

connect_retry_wait
^^^^^^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Wait time before retrying after a connection failure.

**default**: 10s

rsp_header_max_size
^^^^^^^^^^^^^^^^^^^

**optional**, **type**: usize

Maximum response-header size.

**default**: 8192

body_line_max_length
^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: usize

Maximum line length accepted in the response body.

**default**: 512
