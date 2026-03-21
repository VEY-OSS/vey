.. _protocol_egress_path_selection:

#####################
Egress Path Selection
#####################

Egress path selection can be used to control escaper behavior dynamically.

Typical use cases include:

1. Select a specific outgoing IP address

  A single server port may map to multiple outbound IP addresses, and the
  default policy may be random selection. In some cases, users need to choose a
  specific outgoing IP address.
  Instead of creating many separate server and escaper combinations, you can let
  the user provide a custom HTTP header or username parameter that selects the
  desired IP by index.

2. Use dynamic chained proxy for different user

  You can deploy a single server with one ``proxy_float`` escaper and give each
  user a different egress path configuration, allowing different next-hop
  proxies without reloading escapers.

3. Use dynamic chained proxy from client params

  The client can provide the expected proxy address through username
  parameters, then use the ``comply_context`` escaper to derive the egress
  upstream configuration dynamically.

For path selection to work, the selected escaper must both support and enable
the feature. Not all escapers do. Check each escaper's configuration reference
for details.

Server Support
==============

Custom HTTP Header
------------------

Only the HTTP proxy server supports this mechanism.

The supported selection format is :ref:`number id <proto_egress_path_selection_number_id>`.

See :ref:`path_selection_header <config_server_http_proxy_egress_path_selection_header>` for more info.

SOCKS Extension
---------------

Only the SOCKS proxy server could support this mechanism.

There is currently no implementation.

Username Extension
------------------

Any server that supports username-based authentication can use this mechanism.

It sets key-value pairs in the egress context. A ``comply_context`` escaper can
then parse that context and select the egress path for chained escapers.

See :ref:`username_params <config_auth_username_params>` for more info.

User Support
============

User-level egress path selection can be enabled through:

- :ref:`egress_path_id_map <config_user_egress_path_id_map>` for :ref:`string id <proto_egress_path_selection_string_id>` egress path selection

- :ref:`egress_path_value_map <config_user_egress_path_value_map>` for :ref:`json value <proto_egress_path_selection_json_value>` egress path selection

Selection Values
================

The egress path selection data structure contains multiple maps.

In all of these maps, the key is the escaper name, and each escaper reads the
selection value associated with its own name.

The supported value types are described below.

.. _proto_egress_path_selection_number_id:

number id
---------

**value**: map

The value should be a ``usize`` index.

For escapers with multiple nodes, such as multiple next-hop escapers or
multiple outbound IP addresses, the node at the selected index is used.
The index is wrapped into the range ``1 ..= len(nodes)``.

**Note:** indexing starts at ``1``. A value of ``0`` is treated as the last
node.

.. _proto_egress_path_selection_string_id:

string id
---------

**value**: map

The value should be an ID string. Its meaning depends on the escaper type.

.. _proto_egress_path_selection_json_value:

json value
----------

**value**: map

The value should be a JSON map object, or a JSON map string in YAML
configuration. Its meaning depends on the escaper type.

.. _proto_egress_path_selection_egress_upstream:

egress upstream
---------------

**value**: map

The value should be a map with the following keys:

* addr

  **value**: :external+values:ref:`upstream str <conf_value_upstream_str>`

  Overrides the upstream address used by the corresponding escaper.

* resolve_sticky_key

  **value**: string

  Resolves the upstream domain with jump consistent hash and uses this value as
  the hash key.
