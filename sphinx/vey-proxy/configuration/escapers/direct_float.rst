.. _configuration_escaper_direct_float:

************
direct_float
************

This escaper sends traffic directly from the local host, but the candidate bind
addresses are supplied dynamically through the ``publish`` RPC method.

The following interfaces are supported:

* tcp connect
* udp relay
* udp connect
* http(s) forward
* ftp over http

This escaper supports the Cap'n Proto RPC ``publish`` command. The published data must be a map with these keys:

* ipv4

  Set the IPv4 bind IP address or addresses.
  The value may be a single :ref:`bind ip <config_escaper_dynamic_bind_ip>` or an array of them.

* ipv6

  Set the IPv6 bind IP address or addresses.
  The value may be a single :ref:`bind ip <config_escaper_dynamic_bind_ip>` or an array of them.

Published records whose ``expire`` time is already in the past are ignored.

The following egress path selection values are supported:

* :ref:`string id <proto_egress_path_selection_string_id>`

  If matched, the :ref:`bind ip <config_escaper_dynamic_bind_ip>` with the same
  ``id`` is used.

  .. versionadded:: 1.9.2

* :ref:`json value <proto_egress_path_selection_json_value>`

  If matched, the JSON map value is parsed as a
  :ref:`bind ip <config_escaper_dynamic_bind_ip>` and used directly.

  .. versionadded:: 1.9.2

Config Keys
===========

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`resolver <conf_escaper_common_resolver>`, **required**
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`

  The user custom resolve strategy will be taken into account.

* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`udp_sock_speed_limit <conf_escaper_common_udp_sock_speed_limit>`
* :ref:`no_ipv4 <conf_escaper_common_no_ipv4>`
* :ref:`no_ipv6 <conf_escaper_common_no_ipv6>`
* :ref:`tcp_connect <conf_escaper_common_tcp_connect>`

  The user tcp connect params will be taken into account.

* :ref:`happy eyeballs <conf_escaper_common_happy_eyeballs>`
* :ref:`tcp_misc_opts <conf_escaper_common_tcp_misc_opts>`
* :ref:`udp_misc_opts <conf_escaper_common_udp_misc_opts>`

  .. versionadded:: 1.7.22

* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

cache_ipv4
----------

**recommend**, **type**: :external+values:ref:`file path <conf_value_file_path>`

Set the cache file for published IPv4 addresses.

This is recommended because peer discovery at startup may complete only after the first requests arrive.

The file is created if it does not exist.

The cache path is resolved relative to the config file location.

**default**: not set

cache_ipv6
----------

**recommend**, **type**: :external+values:ref:`file path <conf_value_file_path>`

Set the cache file for published IPv6 addresses.

This is recommended because peer discovery at startup may complete only after the first requests arrive.

The file is created if it does not exist.

The cache path is resolved relative to the config file location.

**default**: not set

egress_network_filter
---------------------

**optional**, **type**: :external+values:ref:`egress network acl rule <conf_value_egress_network_acl_rule>`

Set the network filter for the resolved remote IP address.

**default**: all permitted except for loopback and link-local addresses

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Configure TCP keepalive.

User-level TCP keepalive settings are also applied.

**default**: 60s

resolve_redirection
-------------------

**optional**, **type**: :external+values:ref:`resolve redirection <conf_value_resolve_redirection>`

Set DNS redirection rules at the escaper level.

**default**: not set

Publish Format Example
======================

.. code-block:: json

   {
     "ipv4": [
       {
         "ip": "203.0.113.10",
         "id": "hk-v4-a",
         "eip": "198.51.100.10",
         "area": {
           "country": "HK"
         }
       },
       "203.0.113.11"
     ],
     "ipv6": null
   }

.. _config_escaper_dynamic_bind_ip:

Bind IP
=======

Dynamic bind IPs are represented as JSON strings whose root element is a map.

For published data, the root value may also be a bare IP string. Optional
metadata fields with invalid values are ignored instead of rejecting the whole
record.

* ip

  **required**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

  Set the IP address. The address family must match the publish key described above.

.. _config_escaper_dynamic_bind_ip_id:

* id

  **optional**, **type**: str

  Identifier for this bind IP.

  .. versionadded:: 1.7.23

* isp

  **optional**, **type**: str

  ISP for the egress IP address.

* eip

  **optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

  Egress IP address as seen externally.

* area

  **optional**, **type**: :external+values:ref:`egress area <conf_value_egress_area>`

  Area associated with the egress IP address.

* expire

  **optional**, **type**: :external+values:ref:`rfc3339 datetime str <conf_value_rfc3339_datetime_str>`

  Expiration time of this dynamic IP.

  **default**: not set

If every optional field uses its default, the root value can be just the IP itself.

Example:

.. code-block:: json

   {
     "ip": "203.0.113.10",
     "id": "hk-v4-a",
     "isp": "example-isp",
     "eip": "198.51.100.10",
     "area": {
       "country": "HK"
     },
     "expire": "2026-03-28T08:00:00Z"
   }
