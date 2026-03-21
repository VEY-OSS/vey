.. _configure_geoip_value_types:

*****
GeoIP
*****

.. _conf_value_iso_country_code:

iso country code
================

**yaml value**: str

The string must be an ISO 3166 Alpha-2 or Alpha-3 country code.

.. _conf_value_continent_code:

continent code
==============

**yaml value**: str

Supported values are:

  - AF, for Africa
  - AN, for Antarctica
  - AS, for Asia
  - EU, for Europe
  - NA, for North America
  - OC, for Oceania
  - SA, for South America

.. _conf_value_ip_location:

ip location
===========

**type**: map

IP location information.

The supported keys are:

* network

  **required**, **type**: :ref:`ip network str <conf_value_ip_network_str>`

  Registered network address.

* country

  **optional**, **type**: :ref:`iso country code <conf_value_iso_country_code>`

  Country code.

  **default**: not set

* continent

  **optional**, **type**: :ref:`continent code <conf_value_continent_code>`

  Continent code.

  **default**: not set

* as_number

  **optional**, **type**: u32

  AS number.

  **default**: not set

* isp_name

  **optional**, **type**: str

  ISP name.

  **default**: not set

* isp_domain

  **optional**, **type**: str

  ISP domain.

  **default**: not set

.. versionadded:: 1.9.1

.. _conf_value_ip_locate_service:

ip locate service
=================

**type**: map | str

Configuration for the IP-location service.

The supported keys are:

* query_peer_addr

  **optional**, **type**: :ref:`env sockaddr str <conf_value_env_sockaddr_str>`

  Peer UDP socket address.

  **default**: 127.0.0.1:2888

* query_socket_buffer

  **optional**, **type**: :ref:`socket buffer config <conf_value_socket_buffer_config>`

  Socket-buffer configuration for the peer socket.

  **default**: not set

* query_wait_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Timeout for the cache runtime while waiting for a response from the query
  runtime.

  **default**: 1s

.. _conf_value_ip_locate_service_default_expire_ttl:

* default_expire_ttl

  **optional**, **type**: u32

  Default expiration TTL for a response.

  **default**: 10

* maximum_expire_ttl

  **optional**, **type**: u32

  Maximum expiration TTL for a response.

  **default**: 300

* cache_request_batch_count

  **optional**, **type**: usize

  Batch request count used by the cache runtime.

  **default**: 10

* cache_request_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Request timeout seen by the caller.

  **default**: 2s

If the value is a string, it is parsed as ``query_peer_addr`` and all other
fields use their default values.

.. versionadded:: 1.9.1
