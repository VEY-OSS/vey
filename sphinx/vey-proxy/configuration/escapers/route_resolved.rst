.. _configuration_escaper_route_resolved:

route_resolved
==============

This escaper chooses the next escaper from rules that match the resolved
upstream IP address.

There is no path selection support for this escaper.

Resolution follows the Happy Eyeballs algorithm.

The following common keys are supported:

* :ref:`resolver <conf_escaper_common_resolver>`, **required**
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`
* :ref:`default_next <conf_escaper_common_default_next>`

Config-loading rules derived from the implementation:

* ``lpm_match`` also accepts the alias ``lpm_rules``
* duplicate networks across different next escapers are rejected
* each next escaper can appear at most once in ``lpm_match``

lpm_match
---------

**optional**, **type**: seq, **alias**: lpm_rules

If the resolved upstream IP address matches multiple networks, the longest-prefix match is used.

Each rule is in *map* format, with two keys:

* next

  **required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

  Set the next escaper.

* networks

  **optional**, **type**: seq, **alias**: network, net, nets

  Each element should be valid network string. Both IPv4 and IPv6 are supported.

  A network must not appear in rules for different next escapers.

resolution_delay
----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait for the preferred address family after another family has already returned a result.

This has the same meaning as the ``resolution_delay`` field in :external+values:ref:`happy eyeballs <conf_value_happy_eyeballs>`.

**default**: 50ms

Example
-------

.. code-block:: yaml

   resolver: default
   default_next: direct
   lpm_match:
     - next: office-egress
       networks:
         - 203.0.113.0/24
     - next: internal-v6
       net:
         - 2001:db8::/32
