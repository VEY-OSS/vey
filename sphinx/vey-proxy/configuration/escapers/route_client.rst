.. _configuration_escaper_route_client:

route_client
============

This escaper chooses the next escaper from the client IP address.

There is no path selection support for this escaper.

The following common keys are supported:

* :ref:`default_next <conf_escaper_common_default_next>`

Selection order is fixed by the runtime:

* exact IP match first
* then longest-prefix subnet match
* then ``default_next``

The config loader rejects duplicate IPs or subnets across different next
escapers.

exact_match
-----------

**optional**, **type**: seq, **alias**: exact_rules

If the client IP exactly matches an entry in the rules, the corresponding escaper is selected.

Each rule is in *map* format, with two keys:

* next

  **required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

  Set the next escaper.

* ips

  **optional**, **type**: seq, **alias**: ip

  Each element should be :external+values:ref:`ip addr str <conf_value_ip_addr_str>`.

  An IP must not appear in rules for different next escapers.

subnet_match
------------

**optional**, **type**: seq, **alias**: subnet_rules

If the client IP matches multiple subnets, the longest-prefix match is used.

Each rule is in *map* format, with two keys:

* next

  **required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

  Set the next escaper.

* subnets

  **optional**, **type**: seq, **alias**: subnet

  Each element should be :external+values:ref:`ip network str <conf_value_ip_network_str>`.

  A subnet must not appear in rules for different next escapers.

Example
-------

.. code-block:: yaml

   default_next: direct
   exact_match:
     - next: office-egress
       ips:
         - 192.0.2.10
         - 192.0.2.11
   subnet_match:
     - next: vpn-egress
       subnets:
         - 10.0.0.0/8
         - 2001:db8::/32
