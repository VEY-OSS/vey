.. _metrics:

#######
Metrics
#######

Metrics are currently exported through StatsD. See :ref:`stat <configuration_stat>`
for configuration details.

Common Tags
===========

The following are the common tags used by all metrics:

.. _metrics_tag_daemon_group:

* daemon_group

  Same daemon-group value configured in the config file or command-line arguments.

.. _metrics_tag_stat_id:

* stat_id

  A machine-local unique ``stat_id`` used for deduplication. It should be
  **dropped** by the StatsD aggregation pipeline so that metrics with the same
  remaining tags can be aggregated.

.. _metrics_tag_quantile:

* quantile

  Quantile value for histogram statistics.

  The following values are always present:

  - min
  - max
  - mean

  Values can be added by :external+values:ref:`histogram metrics <conf_value_histogram_metrics>` config.
  If not set, the following values are added by default:

  - 0.50
  - 0.80
  - 0.90
  - 0.95
  - 0.99

Metric Types
=============

.. toctree::

   server
