.. _configuration_escaper_route_client:

route_client
============

This escaper selects the next escaper based on the client address.

There is no path selection support for this escaper.

The following common keys are supported:

* :ref:`default_next <conf_escaper_common_default_next>`

exact_match
-----------

**optional**, **type**: seq

If the client IP exactly matches an entry in the rules, the corresponding escaper is selected.

Each rule is in *map* format, with two keys:

* next

  **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

Set the next escaper.

* ips

  **optional**, **type**: seq

  Each element should be :ref:`ip addr str <conf_value_ip_addr_str>`.

  An IP must not appear in rules for different next escapers.

subnet_match
------------

**optional**, **type**: seq

If the client IP matches multiple subnets, the longest-prefix match is used.

Each rule is in *map* format, with two keys:

* next

  **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

  Set the next escaper.

* subnets

  **optional**, **type**: seq

  Each element should be :ref:`ip network str <conf_value_ip_network_str>`.

  A subnet must not appear in rules for different next escapers.
