.. _configuration_backend_stream_tcp:

**********
stream_tcp
**********

A layer-4 backend that connects to upstream peers over TCP.

This backend type is valid only for stream tasks.

Config Keys
===========

The following common keys are supported:

* :ref:`discover <conf_backend_common_discover>`
* :ref:`discover_data <conf_backend_common_discover_data>`
* :ref:`extra_metrics_tags <conf_backend_common_extra_metrics_tags>`

peer_pick_policy
----------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select the next peer address.

For Ketama, rendezvous hash, and jump hash, the hash key is ``<client-ip>``.

**default**: random

duration_stats
--------------

**optional**, **type**: :external+values:ref:`histogram metrics <conf_value_histogram_metrics>`

Configure histogram metrics for TCP connect duration.

**default**: set with default value
