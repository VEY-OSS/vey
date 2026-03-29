.. _config_auth_username_params:

Username Params
===============

Username parameters let you embed structured routing hints in the presented
username.

They do not select an egress path by themselves. The extracted key-value pairs
are copied into the egress context and can then be consumed by a helper such as
:ref:`comply_context <configuration_escaper_comply_context>`. See
:ref:`Egress Path Selection <protocol_egress_path_selection>` for the full
data flow.

The authentication username is the portion before the first recognized
parameter key.

Example configuration:

.. code-block:: yaml

   required_keys:
     - country
   optional_keys:
     - city

If the client sends ``test-me-country-cn-city-none``, the authentication
username becomes ``test-me``.

Parsing notes derived from the implementation:

* The username must contain a non-empty prefix before the first recognized
  parameter key. A string made entirely of parameters is rejected.
* Parameters are parsed as ``<separator><key><separator><value>`` pairs.
  A trailing key without a value is rejected.
* If the same parameter key appears multiple times, the last value wins.
* Unknown keys are either rejected or ignored, depending on
  ``reject_unknown_keys``.

The configuration value is a map with the following keys:

required_keys
-------------

**optional**, **type**: list of string, **alias**: required

Required parameter keys.

The request fails if any required key is missing from the username supplied by
the client.

optional_keys
-------------

**optional**, **type**: list of string, **alias**: optional

Optional parameter keys.

The same key must not appear in both ``required_keys`` and ``optional_keys``.

reject_unknown_keys
-------------------

**optional**, **type**: bool, **alias**: reject_unknown

Set to ``true`` to reject unknown keys, meaning keys that are neither in
``required_keys`` nor in ``optional_keys``.

The request fails if the client-supplied username contains an unknown
parameter key.

**default**: true

param_separator
---------------

**optional**, **type**: char

Separator character used between parameter keys and values.

**default**: '-'

Example
-------

.. code-block:: yaml

   username_params:
     required_keys: [country]
     optional_keys: [city, line]
     reject_unknown_keys: false
     param_separator: '-'

With that configuration, ``alice-country-us-city-sea-extra-ignored`` is
accepted and produces:

* authentication username: ``alice``
* egress context: ``country=us``, ``city=sea``
