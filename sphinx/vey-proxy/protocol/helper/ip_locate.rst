.. _protocol_helper_ip_locate:

=========
IP Locate
=========

We need a peer service in escaper :ref:`route_geoip <configuration_escaper_route_geoip>` config. This page describes the
protocol used to communicate with that peer service.

The peer service should listen on a UDP port over either IPv4 or IPv6.
``vey-proxy`` sends requests to that port.

Each UDP packet sent by ``vey-proxy`` contains exactly one request, and each UDP
packet returned by the peer service should contain exactly one response.

The peer service may also push location responses to ``vey-proxy`` without a
prior request.

Both requests and responses are structured data encoded in `msgpack`_ format.

.. _msgpack: https://msgpack.org/

The top-level object in both the request and the response must be a map. Keys
may be either string keys or numeric key IDs. The supported fields are
described below.

request
=======

ip
--

**required**, **id**: 1, **type**: string

The target IP address.

response
========

ip
--

**optional**, **id**: 1, **type**: string

The target IP address from the request.

This field should be present for a direct response to a request, and omitted for
a pushed response.

ttl
---

**optional**, **id**: 2, **type**: u32

The TTL for the response.

If it is not set, the
:ref:`default expire ttl <conf_value_ip_locate_service_default_expire_ttl>`
configuration is used.

network
-------

**required**, **id**: 3, **type**: :ref:`ip network str <conf_value_ip_network_str>`

The registered network address.

country
-------

**optional**, **id**: 4, **type**: :ref:`iso country code <conf_value_iso_country_code>`

The country code.

continent
---------

**optional**, **id**: 5, **type**: :ref:`continent code <conf_value_continent_code>`

The continent code.

as_number
---------

**optional**, **id**: 6, **type**: u32

The AS number.

isp_name
--------

**optional**, **id**: 7, **type**: str

The ISP name.

isp_domain
----------

**optional**, **id**: 8, **type**: str

The ISP domain.
