.. _configuration_exporter_graphite:

graphite
========

Exporter that sends metrics with the Graphite plaintext protocol.

Configuration
-------------

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`Stream Export Runtime <configuration_exporter_runtime_stream>` is used:

- default port 2003
- all config keys supported

emit_interval
^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s

Metric types
------------

Counters
^^^^^^^^

Each counter is written as its cumulative sum under ``<name>``.

A counter that keeps receiving increments of ``0`` stays in the export. The same sum is written again with a new timestamp. A counter that received no samples during an ``emit_interval`` is omitted for that interval, and the sum starts again on the next sample. Counts from an omitted interval are gone.

Gauges
^^^^^^

Each gauge is written as a single sample under ``<name>``.

Backends
--------

Graphite and VictoriaMetrics accept this plaintext stream.

Graphite
^^^^^^^^

Plot a counter's raw series only when the panel should show the running total. For a per-second rate, apply `perSecond`_. It divides the increase between consecutive samples by the time between their timestamps, and it ignores a decrease. A restarted sum is a decrease, so that step does not become a negative rate.

Use `nonNegativeDerivative`_ when the panel should show how many events arrived between samples. `derivative`_ keeps negative steps, so a restarted sum shows up as a dip.

.. _perSecond: https://graphite.readthedocs.io/en/latest/functions.html#graphite.render.functions.perSecond
.. _nonNegativeDerivative: https://graphite.readthedocs.io/en/latest/functions.html#graphite.render.functions.nonNegativeDerivative
.. _derivative: https://graphite.readthedocs.io/en/latest/functions.html#graphite.render.functions.derivative

::

    perSecond(vey.example.requests)
    perSecond(seriesByTag('name=vey.example.requests'))

Plot a gauge's raw series.

VictoriaMetrics
^^^^^^^^^^^^^^^

Point ``server`` and ``port`` at VictoriaMetrics' Graphite listener. That listener is off unless the process is started with ``-graphiteListenAddr``. ``:2003`` matches this exporter's default port. See the `VictoriaMetrics Graphite integration`_.

.. _VictoriaMetrics Graphite integration: https://docs.victoriametrics.com/victoriametrics/integrations/graphite/

Set a distinct ``global_tags`` value per sender, such as ``host``, so each machine stays on its own series.

Query the per-second rate in MetricsQL with ``rate()`` on each series, then sum:

::

    sum(rate(vey.example.requests[5m]))

``rate()`` treats a decrease as a counter reset. After one machine restarts, its sum starts from zero and the old total is left out of the rate. The summed rate dips by that machine's own traffic. Applying ``rate()`` to the sum of the raw series treats the reset as a reset of the total, and the rate jumps.

Repeated increments of ``0`` hold the sum flat, so ``rate()`` on that series is ``0``. Plot a gauge's raw series.
