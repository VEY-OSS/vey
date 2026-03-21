.. _metrics:

#######
Metrics
#######

``vey-proxy`` currently exports metrics through StatsD only. See
:ref:`stat <configuration_stat>` for configuration details.

Common Tags
===========

The following tags are common to all metrics:

.. _metrics_tag_daemon_group:

* daemon_group

  The daemon group specified in the configuration file or on the command line.

.. _metrics_tag_stat_id:

* stat_id

  A machine-local unique ID used for deduplication. It should be **dropped** by
  StatsD, after which metrics with the same remaining tags should be
  aggregated.

.. _metrics_tag_transport:

* transport

  The transport protocol. Supported values are:

  - tcp
  - udp

.. _metrics_tag_connection:

* connection

  The client connection type. Supported values are:

  - http
  - socks

.. _metrics_tag_request:

* request

  The request type. Supported values are:

  - tcp_connect
  - http_forward
  - https_forward
  - http_connect
  - socks_tcp_connect
  - socks_udp_connect
  - socks_udp_associate

.. _metrics_tag_quantile:

* quantile

  The quantile value for histogram metrics.

  The following values are always present:

  - min
  - max
  - mean

  Additional values can be configured through
  :ref:`histogram metrics <conf_value_histogram_metrics>`.
  If none are configured, the following values are exported by default:

  - 0.50
  - 0.80
  - 0.90
  - 0.95
  - 0.99

Metrics Types
=============

.. toctree::

   server
   escaper
   resolver
   user
   user_site
   logger
   runtime
