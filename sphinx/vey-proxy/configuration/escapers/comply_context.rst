.. _configuration_escaper_comply_context:

**************
comply_context
**************

.. versionadded:: 1.13.0

This escaper parses egress context and sets the egress path for chained escapers.

Config Keys
===========

next
----

**required**, **type**: str

Set the next escaper in the chain.

use_egress_upstream
-------------------

**optional**, **type**: map

Configure a dynamic upstream address derived from the egress context.

The supported keys:

- default_port

  **required**, **type**: u16

  Set the default port.

- host_key

  **optional**, **type**: string

  Context key that provides the domain host.

- port_key

  **optional**, **type**: string

  Context key that provides the upstream port.

- domain_suffix

  **optional**, **type**: domain

  Common domain suffix to append.

  **default**: not set

- resolve_sticky_key

  **optional**, **type**: string

  Context key whose value is used as the hash key when resolving the upstream domain.

  If set and the corresponding value exists, jump consistent hash is used.
