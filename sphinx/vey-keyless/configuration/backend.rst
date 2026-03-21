.. _configuration_backend:

*******
backend
*******

This section configures the keyless backend used to perform private-key
operations.

The backend configuration can be a root-value map as described below, or just a
driver name.

Root Value Map
==============

dispatch_channel_size
---------------------

**optional**, **type**: usize

Channel size used when dispatching requests to worker backends.

This only applies when worker runtimes are enabled in the main config.

**default**: 1024

dispatch_counter_shift
----------------------

**optional**, **type**: u8

Number of requests dispatched to the same worker backend before rotating to the next one.

The effective count is ``2^N``.

This only applies when worker runtimes are enabled in the main config.

**default**: 3

openssl_async_job
-----------------

**optional**, **type**: :ref:`openssl_async_job <conf_backend_driver_openssl_async_job>`

Enable the OpenSSL async-job driver.

**default**: not enabled

Drivers
=======

simple
------

Use the default OpenSSL execution path for private-key operations.

This driver has no additional configuration keys.

.. _conf_backend_driver_openssl_async_job:

openssl_async_job
-----------------

Use OpenSSL async jobs for private-key operations. The hardware crypto engine
or provider can be configured through ``openssl.cnf``.

The following keys are supported for this driver:

- async_op_timeout

  **optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

  Timeout for a single async job.

  A larger value is recommended to avoid edge cases in OpenSSL async-job
  handling.

  **default**: 1s
