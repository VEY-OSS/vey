.. _configuration_escaper_route_failover:

route_failover
==============

.. versionadded:: 1.7.17

This escaper tries a primary next escaper first and falls back to a standby
next escaper when needed.

This escaper has the following limitation:

 - HTTP forwarding is advertised only if both the primary and standby final escapers support it.

There is no path selection support for this escaper.

No common keys are supported.

primary_next
------------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the primary next escaper.

standby_next
------------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the standby next escaper.

fallback_delay
--------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`, **alias**: delay, fallback_timeout, timeout

Set how long to wait before switching to the standby escaper while the primary escaper is still pending.

**default**: 100ms

Example
-------

.. code-block:: yaml

   primary: direct-primary
   standby: direct-standby
   fallback_delay: 250ms
