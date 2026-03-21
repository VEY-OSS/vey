.. _configuration_importer_statsd:

statsd
======

Importer for StatsD-compatible datagrams.

The following common keys are supported:

* :ref:`collector <conf_importer_common_collector>`
* :ref:`listen_in_worker <conf_importer_common_listen_in_worker>`
* :ref:`ingress_network_filter <conf_importer_common_ingress_network_filter>`

listen
------

**optional**, **type**: :external+values:ref:`udp listen <conf_value_udp_listen>`

Listen configuration for this importer.

The listen instance count is ignored when ``listen_in_worker`` is enabled.

**default**: not set
