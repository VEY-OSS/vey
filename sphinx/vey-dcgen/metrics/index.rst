.. _metrics:

#######
Metrics
#######

Metrics are currently exported through StatsD. See :ref:`stat <configuration_stat>`
for configuration details.

The default metric prefix is the executable name.

Common Tags
===========

.. _metrics_tag_daemon_group:

* daemon_group

  Daemon group configured in the config file or with ``-G``. Present on every
  metric.

.. _metrics_tag_stat_id:

* stat_id

  A machine-local unique ``stat_id`` used for deduplication. It should be
  **dropped** by the StatsD aggregation pipeline so that metrics with the same
  remaining tags can be aggregated.

  This tag is present on listen metrics and Tokio runtime metrics. It is not
  present on server request metrics, backend metrics, or allocator metrics.

.. _metrics_tag_server:

* server

  Server name. Present on server request metrics and listen metrics.

.. _metrics_tag_online:

* online

  Whether the server is online. The value is either ``y`` or ``n``. Present on
  listen metrics.

.. _metrics_tag_quantile:

* quantile

  Histogram quantile. Present on ``backend.request_duration``.

Metric Types
============

.. toctree::

   server
   runtime
