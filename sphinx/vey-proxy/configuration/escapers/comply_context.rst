.. _configuration_escaper_comply_context:

**************
comply_context
**************

.. versionadded:: 1.13.0

This is the escaper designed to be used to parse egress context and set egress path for chained escapers.

Config Keys
===========

next
----

**required**, **type**: str

Set the next escaper to be used.

use_egress_upstream
-------------------

**optional**, **type**: map

Set the dynamic upstream address to be used.

The supported keys:

- default_port

  **required**, **type**: u16

  Set the default port to use.

- host_key

  **optional**, **type**: string

  The context key to get domain host.

- port_key

  **optional**, **type**: string

  The context key to get upstream port.

- domain_suffix

  **optional**, **type**: domain

  The common domain suffix.

  **default**: not set

- resolve_sticky_key

  **optional**, **type**: string

  The context key whose value will be used as the hash key when resolving the upstream domain.

  Jump consistent hash will be used if this is set and the corresponding value can be found.
