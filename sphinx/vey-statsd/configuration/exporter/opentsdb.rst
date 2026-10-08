.. _configuration_exporter_opentsdb:

opentsdb
========

Exporter that sends metrics with the OpenTSDB HTTP put protocol.

Configuration
-------------

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`HTTP Export Runtime <configuration_exporter_runtime_http>` is used:

- default port 4242
- all config keys supported

emit_interval
^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s

sync_timeout
^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Controls the ``sync`` and ``sync_timeout`` query parameters.

**default**: not set

max_data_points
^^^^^^^^^^^^^^^

**optional**, **type**: usize

Maximum number of data points sent in a single HTTP request.

**default**: 50

Metric types
------------

Counters
^^^^^^^^

Each counter is one data point. ``value`` is the cumulative sum:

::

    {"metric":"requests","timestamp":1710000000,"value":100,"tags":{"host":"a"}}

A counter that keeps receiving increments of ``0`` stays in the export. The same sum is written again with a new timestamp. A counter that received no samples during an ``emit_interval`` is omitted for that interval, and the sum starts again on the next sample. Counts from an omitted interval are gone.

Gauges
^^^^^^

Each gauge is one data point. ``value`` is the latest sample.

Backends
--------

OpenTSDB and VictoriaMetrics accept ``POST /api/put``.

GreptimeDB accepts the same JSON at ``/v1/opentsdb/api/put``. TDengine accepts it at ``/opentsdb/v1/put/json/<db>``. This exporter always posts ``/api/put``.

OpenTSDB
^^^^^^^^

OpenTSDB serves the `PUT API`_ on port 4242. That matches this exporter's default port.

.. _PUT API: https://opentsdb.net/docs/build/html/api_http/put.html

The stored value is the running total. Ask for a per-second rate and drop a decrease, so a restarted sum is skipped instead of treated as a rollover:

::

    {
      "aggregator": "sum",
      "metric": "requests",
      "rate": true,
      "rateOptions": {
        "counter": true,
        "dropResets": true
      }
    }

Plot a gauge's raw series.

VictoriaMetrics
^^^^^^^^^^^^^^^

Point ``server`` and ``port`` at VictoriaMetrics' OpenTSDB HTTP listener. That listener is off unless the process is started with ``-opentsdbHTTPListenAddr``. ``:4242`` matches this exporter's default port. See the `VictoriaMetrics OpenTSDB integration`_.

.. _VictoriaMetrics OpenTSDB integration: https://docs.victoriametrics.com/victoriametrics/integrations/opentsdb/

The metric name and tags are stored as written. Set a distinct ``global_tags`` value per sender, such as ``host``, so each machine stays on its own series.

Query the per-second rate in MetricsQL with ``rate()`` on each series, then sum:

::

    sum(rate(requests[5m]))

``rate()`` treats a decrease as a counter reset. After one machine restarts, its sum starts from zero and the old total is left out of the rate. The summed rate dips by that machine's own traffic. Applying ``rate()`` to the sum of the raw series treats the reset as a reset of the total, and the rate jumps.

Repeated increments of ``0`` hold the sum flat, so ``rate()`` on that series is ``0``. Plot a gauge's raw series.
