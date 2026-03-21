.. _metrics:

#######
Metrics
#######

``vey-gateway`` currently exports metrics through StatsD. See
:ref:`stat <configuration_stat>` for configuration details.

Common Tags
===========

The following tags are shared by all metrics:

.. _metrics_tag_daemon_group:

* daemon_group

  This tag matches the daemon group set in the configuration file or on the
  command line.

.. _metrics_tag_stat_id:

* stat_id

  A host-local unique ID used for deduplication. StatsD should **drop** this
  tag so metrics with the same remaining tags can be aggregated.

.. _metrics_tag_transport:

* transport

  The transport-layer protocol. Values are:

  - tcp
  - udp

.. _metrics_tag_quantile:

* quantile

  The quantile label for histogram metrics.

  The following values are always present:

  - min
  - max
  - mean

  Additional quantiles can be configured with
  :external+values:ref:`histogram metrics <conf_value_histogram_metrics>`.
  If not set, the following values are added by default:

  - 0.50
  - 0.80
  - 0.90
  - 0.95
  - 0.99

Metrics Types
=============

.. toctree::

   server
   logger
   backend/index
   runtime
