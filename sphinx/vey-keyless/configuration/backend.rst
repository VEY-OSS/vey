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

crypto_mb
---------

**optional**, **type**: :ref:`crypto_mb <conf_backend_driver_crypto_mb>`

Enable the Intel ``crypto_mb`` multi-buffer driver.

This requires building ``vey-keyless`` with the ``crypto-mb`` feature on
``x86_64`` and a system ``libcrypto_mb``.

**default**: not enabled

.. versionadded:: 0.6.0

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

.. _conf_backend_driver_crypto_mb:

crypto_mb
---------

Use Intel ``crypto_mb`` multi-buffer primitives for RSA-2048/3072/4096, ECDSA
(P-256/P-384/P-521), and Ed25519.

Requests are taken with ``recv_many`` up to 8. A single request is handled by
OpenSSL; two or more requests in the same batch use ``crypto_mb``.

This driver requires worker runtimes and a multiplex-enabled ``cloudflare``
server so requests are dispatched to backend workers.

The driver map currently has no additional keys; ``crypto_mb: {}`` or the
string form ``backend: crypto_mb`` is enough.

This driver is only available on ``x86_64`` when ``vey-keyless`` is built with
the ``crypto-mb`` feature.

If the CPU lacks a supported ISA for ``crypto_mb`` (for example AVX-512 IFMA
or AVX2-IFMA), the daemon logs a warning and falls back to OpenSSL for every
request.

.. versionadded:: 0.6.0
