.. _configure_metrics_value_types:

*******
Metrics
*******

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: available
   - ``vey-statsd``: available

.. _conf_value_metric_value:

metric value
============

**yaml value**: limited str

Only the following characters are allowed:

a to z, A to Z, 0 to 9, -, _, ., / or Unicode letters (as per the specification)

The character range is the same as `OpenTSDB metrics-and-tags`_.

.. _OpenTSDB metrics-and-tags: http://opentsdb.net/docs/build/html/user_guide/writing/index.html#metrics-and-tags

.. _conf_value_metric_tag_name:

metric tag name
===============

**yaml value**: :ref:`metric value <conf_value_metric_value>`

Metric tag name. It must not be empty.

.. _conf_value_metric_tag_value:

metric tag value
================

**yaml value**: :ref:`metric value <conf_value_metric_value>`

Metric tag value. It may be empty depending on the context.

.. _conf_value_static_metrics_tags:

static metrics tags
===================

**yaml value**: map

Keys must be :ref:`metric tag names <conf_value_metric_tag_name>`.
Values must be :ref:`metric tag values <conf_value_metric_tag_value>`.

Tag values may be strings, integers, or floating-point YAML values, as long as
their string form is a valid metric tag value.

.. _conf_value_metric_node_name:

metric node name
================

**yaml value**: :ref:`metric value <conf_value_metric_value>`

Metric node name.

.. _conf_value_metric_name_prefix:

metric name prefix
==================

**yaml value**: seq of :ref:`metric node name <conf_value_metric_node_name>` | str

Metric-name prefix.

This may be a sequence of metric node names or a single ``.``-delimited
string.

.. availability::

   - ``vey-statsd``: available in ``0.2.0`` and later
   - ``vey-proxy``: not currently used
   - ``vey-gateway``: not currently used
   - ``vey-keyless``: not currently used

.. _conf_value_weighted_metric_node_name:

weighted metric node name
=========================

**yaml value**: map | :ref:`metric node name <conf_value_metric_node_name>`

A weighted metric node name suitable for use inside a selective vector.

The map consists of two fields:

* name

  **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

  Node name. Its meaning depends on the configuration context.

* weight

  **optional**, **type**: f64

  Weight assigned to the node name.
  When used internally, it may be converted to the smallest ``u32`` greater
  than or equal to the ``f64`` value.

  **default**: 1.0

If the value is a string, it is treated as the ``name`` field and ``weight``
uses the default value.

Example:

.. code-block:: yaml

   next_nodes:
     - name: direct-a
       weight: 2.0
     - direct-b

.. _conf_value_metrics_quantile:

metrics quantile
================

**yaml value**: str | float

A quantile value in the range ``0.0`` to ``1.0``.

When configured as a string, that exact string is used as the ``quantile`` tag
value. Use the string form when you want the exported tag value to match the
configuration exactly.

Example: ``"0.950"`` preserves the trailing zero in the exported metric tag,
while ``0.95`` does not.

.. _conf_value_histogram_metrics:

histogram metrics
=================

**yaml value**: map | :ref:`rotate <conf_value_histogram_metrics_rotate>`

Histogram-metric configuration, including quantiles and rotation interval.

The keys are:

quantile
--------

**optional**, **type**: seq

List of quantiles to export.

This can be either a sequence of
:ref:`metrics quantile <conf_value_metrics_quantile>` values or a comma-separated
string.

**default**: 0.50, 0.80, 0.90, 0.95, 0.99

.. _conf_value_histogram_metrics_rotate:

rotate
------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Rotation interval.

**default**: 4s

Example:

.. code-block:: yaml

   histogram_metrics:
     quantile: "0.50,0.90,0.99"
     rotate: 10s

.. _conf_value_statsd_client_config:

Statsd Client Config
====================

The full root value is a map with the following keys:

target_unix
-----------

**optional**, **type**: mix

Use this to send StatsD metrics to a custom UNIX-domain socket path.

The value can be a map, with the following keys:

* path

  **required**, **type**: :ref:`absolute path <conf_value_absolute_path>`

  UNIX-domain socket path.

If the value type is str, the value should be the same as the value as *path* above.

**default**: not set

target_udp
----------

**optional**, **type**: mix

Use this to send StatsD metrics to a remote StatsD server listening on UDP.

The value can be a map, with the following keys:

* address

  **optional**, **type**: :ref:`env sockaddr str <conf_value_env_sockaddr_str>`

  Remote socket address.

  **default**: 127.0.0.1:8125

* bind_ip

  **optional**, **type**: :ref:`ip addr str <conf_value_ip_addr_str>`

  Local IP address to bind before sending metrics.

  **default**: not set

If the value type is str, the value should be the same as the value as *address* above.

target
------

**optional**, **type**: map

Alternative form for specifying the StatsD target.

The key *udp* is just handled as *target_udp* as above.

The key *unix* is just handled as *target_unix* as above.

prefix
------

**optional**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

Global prefix applied to all metrics.

**default**: "vey-proxy"

cache_size
----------

**optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

Batch cache size used before sending metrics to the backend.

**default**: 256KiB

.. availability::


   - ``vey-proxy``: available since ``1.11.8``

max_segment_size
----------------

**optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

Maximum segment size used when sending data to the backend.

**default**: 1400 for UDP Socket, 4096 for UNIX Datagram Socket

.. availability::


   - ``vey-proxy``: available since ``1.11.8``

emit_interval
-------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for local metrics. All metrics are sent in sequence.

**default**: 200ms

.. availability::


   - ``vey-proxy``: available since ``1.11.8``

emit_duration
-------------

**deprecated**

.. availability::


   - ``vey-proxy``: changed in ``1.11.8``: deprecated, use emit_interval instead
