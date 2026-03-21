.. _configuration_escaper_route_resolved:

route_resolved
==============

This escaper selects the next escaper based on rules applied to the resolved upstream IP address.

There is no path selection support for this escaper.

Resolution follows the Happy Eyeballs algorithm.

The following common keys are supported:

* :ref:`resolver <conf_escaper_common_resolver>`, **required**
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`
* :ref:`default_next <conf_escaper_common_default_next>`

lpm_match
---------

**optional**, **type**: seq

If the resolved upstream IP address matches multiple networks, the longest-prefix match is used.

Each rule is in *map* format, with two keys:

* next

  **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

  Set the next escaper.

* networks

  **optional**, **type**: seq

  Each element should be valid network string. Both IPv4 and IPv6 are supported.

  A network must not appear in rules for different next escapers.

resolution_delay
----------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

How long to wait for the preferred address family after another family has already returned a result.

This has the same meaning as the ``resolution_delay`` field in :ref:`happy eyeballs <conf_value_happy_eyeballs>`.

**default**: 50ms
