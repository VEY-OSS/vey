.. _configuration_escaper_comply_context:

**************
comply_context
**************

.. versionadded:: 1.13.0

This escaper reads values from the egress context and turns them into
path-selection values for downstream escapers.

It does not make the final routing decision itself. It updates path-selection
values in :ref:`the per-request egress path structure <protocol_egress_path_selection>`
and then forwards the request to ``next``.

Config Keys
===========

next
----

**required**, **type**: str

Set the next escaper in the chain.

use_egress_upstream
-------------------

**optional**, **type**: map | list

Configure a (list of) dynamic upstream address derived from the egress context.

At runtime, each entry reads values from the egress context and may emit an
``egress upstream`` override for the named escaper.

The supported keys:

- escaper

  **required**, **type**: str

  The escaper to apply this egress upstream config.

- default_port

  **required**, **type**: u16

  Set the default port used when ``port_key`` is absent or invalid.

- host_key

  **optional**, **type**: string

  Context key that provides the upstream host.

  If ``domain_suffix`` is set, the value from ``host_key`` is treated as a host
  label and the suffix is appended before validation.

- port_key

  **optional**, **type**: string

  Context key that provides the upstream port.

  If this key is not present or cannot be parsed as ``u16``, ``default_port``
  is used.

- domain_suffix

  **optional**, **type**: domain

  Common domain suffix to append.

  **default**: not set

- resolve_sticky_key

  **optional**, **type**: string

  Context key whose value is used as the hash key when resolving the upstream domain.

  If set and the corresponding value exists, jump consistent hash is used.

If ``host_key`` is missing or invalid, no upstream address override is emitted.
If only ``resolve_sticky_key`` is present, only the sticky key is emitted.

Example:

.. code-block:: yaml

   use_egress_upstream:
     - escaper: next-proxy
       host_key: proxy_host
       port_key: proxy_port
       default_port: 3128
       domain_suffix: corp.example.net
       resolve_sticky_key: session_id

use_egress_index
----------------

**optional**, **type**: map | list

Configure one or more egress path selection entries derived from the egress
context.

At runtime:

* ``number_index_key`` is used only if the context value parses as ``usize``
* ``string_index_key`` copies the raw string value as-is
* invalid or missing values are ignored instead of failing the request

The supported keys:

- escaper

  **required**, **type**: str

  The escaper to apply this egress index config.

- number_index_key

  **optional**, **type**: string

  Match this key and set egress :ref:`number id <proto_egress_path_selection_number_id>`.

- string_index_key

  **optional**, **type**: string

  Match this key and set egress :ref:`string id <proto_egress_path_selection_string_id>`.

Example:

.. code-block:: yaml

   use_egress_index:
     - escaper: direct-egress
       number_index_key: bind_slot
     - escaper: proxy-pool
       string_index_key: peer_id
