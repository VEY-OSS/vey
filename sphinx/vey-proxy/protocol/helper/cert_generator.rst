.. _protocol_helper_cert_generator:

=====================
Certificate Generator
=====================

We need a peer service in auditor :ref:`tls cert agent <conf_auditor_tls_cert_agent>` config. This page describes the
protocol used to communicate with that peer service.

The peer service should listen on a UDP port over either IPv4 or IPv6.
``vey-proxy`` sends requests to that port.

Each UDP packet sent by ``vey-proxy`` contains exactly one request, and each UDP
packet returned by the peer service should contain exactly one response.

Both requests and responses are structured data encoded in `msgpack`_ format.

.. _msgpack: https://msgpack.org/

The top-level object in both the request and the response must be a map. Keys
may be either string keys or numeric key IDs. The supported fields are
described below.

request
=======

host
----

**required**, **id**: 1, **type**: string

The hostname of the target TLS server. It may be either a domain name or an IP
address.

service
-------

**optional**, **id**: 2, **type**: string | u8

The TLS service type. The same value should be echoed back in the response.

**default**: http

.. versionadded:: 1.9.0

usage
-----

**optional**, **id**: 4, **type**: string | u8

Set the tls certificate usage type. It should be returned in response.

**default**: tls_server

.. versionadded:: 1.9.1

cert
----

**optional**, **id**: 3, **type**: pem string or der binary

The real upstream leaf certificate, in either PEM string format or DER binary
format.

.. versionadded:: 1.9.0

response
========

host
----

**required**, **id**: 1, **type**: string

The hostname provided in the request.

service
-------

**optional**, **id**: 2, **type**: string | u8

The TLS service type. It should be the same value that was sent in the request.

**default**: http

.. versionadded:: 1.9.0

usage
-----

**optional**, **id**: 6, **type**: string | u8

The TLS certificate usage type. It should be the same value that was sent in
the request.

**default**: tls_server

.. versionadded:: 1.9.1

cert
----

**required**, **id**: 3, **type**: pem string

The generated interception certificate chain in PEM format.

key
---

**required**, **id**: 4, **type**: pem string or der binary

The generated private key, in either PEM string format or DER binary format.

ttl
---

**optional**, **id**: 5, **type**: u32

The TTL for this response.

If the value is ``0``, the
:external+values:ref:`protective cache ttl <conf_value_dpi_tls_cert_agent_protective_cache_ttl>`
configuration is used.

.. note:: Expired records remain cached for a short additional period before
   being removed. See
   :external+values:ref:`cache_vanish_wait <conf_value_dpi_tls_cert_agent_cache_vanish_wait>`
   for details.

**default**: 0
