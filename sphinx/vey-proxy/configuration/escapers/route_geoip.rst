.. _configuration_escaper_route_geoip:

route_geoip
===========

This escaper chooses the next escaper by applying GeoIP rules to the resolved
upstream IP address.

There is no path selection support for this escaper.

Resolution follows the Happy Eyeballs algorithm.

The following common keys are supported:

* :ref:`resolver <conf_escaper_common_resolver>`, **required**
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`
* :ref:`default_next <conf_escaper_common_default_next>`

Config-loading rules derived from the implementation:

* ``geo_rules`` also accepts the alias ``geo_match``
* duplicate networks, ASNs, countries, or continents across different next
  escapers are rejected
* each next escaper can appear at most once for each rule category

ip_locate_service
-----------------

**optional**, **type**: :external+values:ref:`ip locate service <conf_value_ip_locate_service>`

Set the configuration of the remote IP location service.

**default**: set with default config

.. versionadded:: 1.9.1

geo_rules
---------

**optional**, **type**: seq, **alias**: geo_match

Set the GeoIP rules used to select the next escaper.

Each rule is in *map* format, with the following keys:

* next

  **required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

  Set the next escaper.

* networks

  **optional**, **type**: :external+values:ref:`ip network <conf_value_ip_network_str>` | seq

  Each element should be valid network string. Both IPv4 and IPv6 are supported.

  A network must not appear in rules for different next escapers.

* as_numbers

  **optional**, **type**: u32 | seq

  Each element should be valid AS number.

  An AS number must not appear in rules for different next escapers.

* countries

  **optional**, **type**: :external+values:ref:`iso country code <conf_value_iso_country_code>` | seq

  Each element should be valid ISO country code.

  A country must not appear in rules for different next escapers.

* continents

  **optional**, **type**: :external+values:ref:`continent code <conf_value_continent_code>` | seq

  Each element should be valid continent code.

  A continent must not appear in rules for different next escapers.

Example:

.. code-block:: yaml

   geo_rules:
     - next: cn-exit
       countries: [CN]
     - next: eu-exit
       continents: [EU]
     - next: corp-net
       networks:
         - 10.0.0.0/8

resolution_delay
----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait for the preferred address family after another family has already returned a result.

This has the same meaning as the ``resolution_delay`` field in :external+values:ref:`happy eyeballs <conf_value_happy_eyeballs>`.

**default**: 50ms
