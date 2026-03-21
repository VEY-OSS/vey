.. _protocol_helper_route_query:

===========
Route Query
===========

We need a peer service in :ref:`route_query escaper <configuration_escaper_route_query>`. This page describes the
protocol used to communicate with that peer service.

The peer service should listen on a UDP port over either IPv4 or IPv6.
``vey-proxy`` sends requests to that port.

Each UDP packet sent by ``vey-proxy`` contains exactly one request, and each UDP
packet returned by the peer service should contain exactly one response.

Both requests and responses are structured data encoded in `msgpack`_ format.

.. _msgpack: https://msgpack.org/

The top-level object in both the request and the response must be a map. The
supported fields are described below.

request
=======

id
--

**required**, **type**: uuid binary

The request ID.

user
----

**required**, **type**: string

The username associated with the proxy request. This value may be an empty
string if authentication is disabled on the proxy side.

host
----

**required**, **type**: string

The target host of the proxy request. It may be either a domain name or an IP
address.

client_ip
---------

The client IP address. This field is present only when
:ref:`query_pass_client_ip <configuration_escaper_route_query_pass_client_ip>`
is enabled.

response
========

id
--

**required**, **type**: uuid binary | uuid string

The ID of the corresponding request.

nodes
-----

**optional**, **type**: string | seq

The candidate next-hop escaper or escapers that may be selected.

If the value is a sequence, each element must be a
:ref:`weighted metric node name <conf_value_weighted_metric_node_name>`.

If the value is empty, the
:ref:`fallback node <configuration_escaper_route_query_fallback_node>`
configuration is used.

**default**: empty

ttl
---

**optional**, **type**: u32

The TTL for this response.

If the value is ``0``, the
:ref:`protective cache ttl <configuration_escaper_route_query_protective_cache_ttl>`
configuration is used.

.. note:: Expired records remain cached for a short additional period before
   being removed. See
   :ref:`cache_vanish_wait <configuration_escaper_route_query_cache_vanish_wait>`
   for details.

**default**: 0
