.. _config_auth_username_params:

Username Params
===============

The username params can be used to set egress context KVs used in egress path selection.

The username for auth will be all of the chars before the first known param key.

Example Config:

```yaml
required_keys: country
optional_keys: city
```

When we received `test-me-country-cn-city-none` from the client we will used `test-me` as the auth username.

The config value should be a map, the keys are:

required_keys
-------------

**optional**, **type**: list of string

Set the required param keys.

The request will fail if any of the required keys is missing in the username sent from the client.

optional_keys
-------------

**optional**, **type**: list of string

Set the optional param keys.

reject_unknown_keys
-------------------

**optional**, **type**: bool

Set to true if you want to reject unknown keys (neither in required_keys nor in optional_keys).

The request will fail if any of the param key in the username sent from the client is unknown.

**default**: false

param_separator
---------------

**optional**, **type**: char

Set the separator char for each params key and value.

**default**: '-'
