.. _configuration_resolver_fail_over:

fail_over
=========

Resolver wrapper that queries a primary resolver first and falls back to a
standby resolver when needed.

At runtime the result is chosen with the following rules:

1. The **success** result of the primary resolver will always be used before the timeout.
2. The first **success** result either from the primary or the standby resolver will be used after the timeout.
3. If no resolver returns a successful result, the last error is used.

The following common keys are supported:

* :ref:`graceful_stop_wait <conf_resolver_common_graceful_stop_wait>`
* :ref:`protective_query_timeout <conf_resolver_common_protective_query_timeout>`

The config loader rejects using the same resolver name for both ``primary`` and
``standby``.

primary
-------

**required**, **type**: string

Primary resolver.

standby
-------

**required**, **type**: string

Standby resolver.

fallback_delay
--------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`, **alias**: delay, fallback_timeout, timeout

Timeout before the standby resolver is allowed to participate.

**default**: 100ms

negative_ttl
------------

**optional**, **type**: u32, **alias**: protective_cache_ttl

Time-to-Live (TTL) for negative caching of failed DNS lookups.

**default**: 30

retry_empty_record
------------------

**optional**, **type**: bool

Controls whether a fallback query should be attempted when the first answer
contains no IP addresses.

**default**: false

Example
-------

.. code-block:: yaml

   primary: hickory-main
   standby: c-ares-backup
   fallback_delay: 250ms
   negative_ttl: 30
   retry_empty_record: true

.. versionadded:: 1.7.13
