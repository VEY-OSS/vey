.. _protocol_egress_path_selection:

#####################
Egress Path Selection
#####################

Egress path selection lets the client or user configuration influence how an
escaper sends traffic to the next hop. The exact behavior depends on the
escaper. Typical uses include:

1. Selecting one bind IP from a pool exposed by a direct escaper

   One server port may map to multiple outbound IP addresses. Instead of
   creating many server and escaper combinations, a request can carry a
   selection value that chooses the desired egress node.

2. Selecting a per-user chained proxy

   A single server can use one dynamic escaper such as ``proxy_float`` while
   each user selects a different published peer.

3. Supplying the next-hop proxy from request metadata

   Request metadata can be copied into the egress context and then transformed
   into an :ref:`egress upstream <proto_egress_path_selection_egress_upstream>`
   value for chained proxy escapers.

The selected escaper must support the value type being used, and some escapers
also require path selection to be enabled in their own configuration.

Data Flow
=========

Path selection data is always evaluated per escaper. The resulting structure is
a map whose keys are escaper names. Each escaper reads only the value stored
under its own name.

The data can reach that structure in two ways:

1. Server-side request metadata

   The server extracts values from protocol-specific input and stores them in
   the egress context. A helper escaper such as
   :ref:`comply_context <configuration_escaper_comply_context>` can then
   translate those context keys into egress path selection values.

2. User configuration

   A user can define per-escaper selections directly with
   :ref:`egress_path_id_map <config_user_egress_path_id_map>` or
   :ref:`egress_path_value_map <config_user_egress_path_value_map>`.

Server Support
==============

Custom HTTP Header
------------------

This mechanism is currently implemented only by the HTTP proxy server.

The HTTP proxy server can:

* copy selected request headers into the egress context with
  :ref:`egress_context_headers <config_server_http_proxy_egress_context_headers>`
* map one request header directly to a
  :ref:`number id <proto_egress_path_selection_number_id>` with
  :ref:`egress_path_selection_header <config_server_http_proxy_egress_path_selection_header>`

This is useful when the client can set HTTP headers but cannot change the
authenticated username.

SOCKS Extension
---------------

The SOCKS proxy server could support a protocol extension for path selection,
but there is currently no implementation.

Username Parameters
-------------------

Any server that supports username-based authentication can populate the egress
context from username parameters.

This does not select an egress path by itself. It only adds key-value pairs to
the egress context. A ``comply_context`` escaper can then convert those values
to path selection entries.

See :ref:`username_params <config_auth_username_params>` for the parsing rules.

User Support
============

User-level path selection is configured per user and per escaper:

* :ref:`egress_path_id_map <config_user_egress_path_id_map>` stores
  :ref:`string id <proto_egress_path_selection_string_id>` values

* :ref:`egress_path_value_map <config_user_egress_path_value_map>` stores
  :ref:`json value <proto_egress_path_selection_json_value>` values

Selection Values
================

The supported value types are described below. In every case, the outer map key
is the escaper name.

.. _proto_egress_path_selection_number_id:

number id
---------

**value**: map of ``<escaper-name>`` to integer

The value is a ``usize`` node selector.

For escapers with multiple nodes, such as multiple next-hop escapers or
multiple outbound IP addresses, the node at the selected index is used.
The index is wrapped into the range ``1 ..= len(nodes)``.

Indexing starts at ``1``. A value of ``0`` selects the last node.

.. _proto_egress_path_selection_string_id:

string id
---------

**value**: map of ``<escaper-name>`` to string

The string is an escaper-defined identifier. Its meaning depends on the escaper
type.

.. _proto_egress_path_selection_json_value:

json value
----------

**value**: map of ``<escaper-name>`` to JSON object

The value must be a JSON map object. In YAML configuration, it can also be
written as a JSON map string. Its meaning depends on the escaper type.

.. _proto_egress_path_selection_egress_upstream:

egress upstream
---------------

**value**: map of ``<escaper-name>`` to upstream override

This is a specialized JSON value used by chained proxy escapers to override the
next-hop proxy target. The inner value is a map with the following keys:

* addr

  **value**: :external+values:ref:`upstream str <conf_value_upstream_str>`

  Override the upstream address used by the corresponding escaper.

* resolve_sticky_key

  **optional**, **value**: string

  Resolve the upstream domain with jump consistent hash and use this value as
  the hash key.
