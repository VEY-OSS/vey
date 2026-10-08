.. _configuration_exporter_prometheus_push:

prometheus_push
===============

Exporter that pushes metrics with Prometheus remote write 1.0.

The request body is one Snappy block-compressed ``WriteRequest``. Each series is one sample.
A counter sample is the cumulative sum. A gauge sample is the current value.

Configuration
-------------

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`HTTP Export Runtime <configuration_exporter_runtime_http>` is used:

- default port 9090
- all config keys supported

emit_interval
^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s

max_samples
^^^^^^^^^^^

**optional**, **type**: usize

Maximum number of samples in one request.

**default**: 10000

path
^^^^

**optional**, **type**: string

Request path. Prometheus and VictoriaMetrics use ``/api/v1/write``. Mimir and Cortex use ``/api/v1/push``.

**default**: ``/api/v1/write``

bearer_token
^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Sent as ``Authorization: Bearer <bearer_token>``.

**default**: not set

org_id
^^^^^^

**optional**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Sent as ``X-Scope-OrgID``. Mimir and Cortex use this header as the tenant id.

**default**: not set

Metric types
------------

Counters
^^^^^^^^

Each counter is one sample. The value is the cumulative sum.

A counter that keeps receiving increments of ``0`` stays in the export. The same sum is written again with a new timestamp. A counter that received no samples during an ``emit_interval`` is omitted for that interval, and the sum starts again on the next sample. Counts from an omitted interval are gone.

Gauges
^^^^^^

Each gauge is one sample. The value is the latest sample.

Names
-----

``__name__`` is the metric name. A metric name is a list of nodes, not a dot-separated string. This exporter joins the prefix nodes and the name nodes with ``_``. A StatsD name ``vey.example.requests`` is the nodes ``vey``, ``example``, and ``requests``, written as ``vey_example_requests``. A character inside a node outside ``[A-Za-z0-9_:]`` becomes ``_``. A name that starts with a digit gains a leading ``_``.

Label names use the same folding, without ``:``. A label name that starts with ``__`` gains a ``key_`` prefix. ``__name__`` stays the metric name. Label values are unchanged.

Folding can merge two names. Nodes ``foo`` and ``bar`` join as ``foo_bar``, the same series as a node that is already ``foo_bar``. ``a-b`` and ``a_b`` are one label; the value from the later original name is kept.

Backends
--------

Query the per-second rate with ``rate()`` on each series. ``rate()`` treats a decrease as a reset. After the sum starts again from zero, the old total is left out of the rate. Plot a gauge's raw series.

Prometheus
^^^^^^^^^^

Prometheus accepts remote write when it is started with ``--web.enable-remote-write-receiver``. The receiver is ``/api/v1/write`` on the Prometheus port. Port 9090 matches this exporter's default.

VictoriaMetrics
^^^^^^^^^^^^^^^

VictoriaMetrics accepts remote write on its HTTP listener. Single-node uses port 8428. Set ``port`` to match. The path stays ``/api/v1/write``. See the `VictoriaMetrics Prometheus remote write integration`_.

.. _VictoriaMetrics Prometheus remote write integration: https://docs.victoriametrics.com/victoriametrics/integrations/prometheus/

Set a distinct ``global_tags`` value per sender, such as ``host``, so each machine stays on its own series.

::

    sum(rate(vey_example_requests[5m]))

Mimir and Cortex
^^^^^^^^^^^^^^^^

Set ``path`` to ``/api/v1/push``. Set ``org_id`` to the tenant. Leave ``bearer_token`` unset when the listener does not check ``Authorization``.
