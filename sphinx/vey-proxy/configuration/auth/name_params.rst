.. _config_auth_username_params:

Username Params
===============

Username parameters can be used to populate egress-context key-value pairs for
egress path selection.

The authentication username is the portion before the first recognized
parameter key.

Example configuration:

```yaml
required_keys: country
optional_keys: city
```

If the client sends ``test-me-country-cn-city-none``, the authentication
username becomes ``test-me``.

The configuration value is a map with the following keys:

required_keys
-------------

**optional**, **type**: list of string

Required parameter keys.

The request fails if any required key is missing from the username supplied by
the client.

optional_keys
-------------

**optional**, **type**: list of string

Optional parameter keys.

reject_unknown_keys
-------------------

**optional**, **type**: bool

Set to ``true`` to reject unknown keys, meaning keys that are neither in
``required_keys`` nor in ``optional_keys``.

The request fails if the client-supplied username contains an unknown
parameter key.

**default**: false

param_separator
---------------

**optional**, **type**: char

Separator character used between parameter keys and values.

**default**: '-'
