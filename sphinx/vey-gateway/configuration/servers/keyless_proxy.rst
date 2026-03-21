.. _configuration_server_keyless_proxy:

keyless_proxy
=============

A reverse-proxy server for the keyless protocol.

The following common keys are supported:

* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

backend
-------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the backend used to handle accepted keyless tasks.

spawn_task_unconstrained
------------------------

**optional**, **type**: bool

Set whether task futures should be spawned with Tokio's unconstrained mode.

**default**: false
