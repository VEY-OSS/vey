.. _configure_route_value_types:

*****
Route
*****

.. _conf_value_host_matched_object:

Host Matched Object
===================

**yaml value**: map | seq of map

Host-based match object for a generic type ``T``, as referenced by specific
configuration options.

The YAML value for ``T`` is still a map, but the following keys are reserved for
matching logic:

* exact_match

  **optional**, **type**: :ref:`host <conf_value_host>`

  Matches when the target host is exactly this host.

* child_match

  **optional**, **type**: :ref:`domain <conf_value_domain>`

  Matches when the target host is a child domain of this parent domain.

* set_default

  **optional**, **type**: bool

  If ``true``, also use this ``T`` as the default value.

  **default**: false

If none of the reserved keys are present, the parsed ``T`` value is also used
as the default value.

A match object can contain one or more ``T`` values, so the YAML value may be a
single ``T`` or a sequence of ``T`` values.

Only one ``T`` is allowed for each match rule, including the default rule.

.. _conf_value_uri_path_matched_object:

Uri Path Matched Object
=======================

**yaml value**: map | seq of map

URI-path-based match object for a generic type ``T``, as referenced by specific
configuration options.

The YAML value for ``T`` is still a map, but the following keys are reserved for
matching logic:

* prefix_match

  **optional**, **type**: str

  Matches when the target URI path has this prefix.

* set_default

  **optional**, **type**: bool

  If ``true``, also use this ``T`` as the default value.

  **default**: false

If none of the reserved keys are present, the parsed ``T`` value is also used
as the default value.

A match object can contain one or more ``T`` values, so the YAML value may be a
single ``T`` or a sequence of ``T`` values.

Only one ``T`` is allowed for each match rule, including the default rule.
