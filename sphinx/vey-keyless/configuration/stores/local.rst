.. _configuration_store_local:

local
=====

This local store loads private keys from a local directory.

The following keys are supported:

directory
---------

**required**, **type**: :external+values:ref:`directory path <conf_value_directory_path>`

Path to the local directory that contains the private keys.

watch
-----

**optional**, **type**: bool

Enable write watching for ``.key`` files under the store directory.

Newly written keys are loaded automatically after a completed write event is observed.

This is only supported on Linux.

**default**: false
