.. _configuration_resolver:

********
Resolver
********

Each resolver configuration item is a map with two required keys:

* :ref:`name <conf_resolver_common_name>`, which defines the resolver name
* :ref:`type <conf_resolver_common_type>`, which selects the concrete resolver
  type and therefore determines how the remaining keys are interpreted

The available resolver types are documented below.

Resolvers
=========

.. toctree::
   :maxdepth: 1

   deny_all
   fail_over
   c_ares
   hickory

Common Keys
===========

This section describes common keys shared by many resolver types.

Most of these settings apply to the standalone resolver runtime.

.. _conf_resolver_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

The resolver name.

.. _conf_resolver_common_type:

type
----

**required**, **type**: str

The resolver type.

.. _conf_resolver_common_graceful_stop_wait:

graceful_stop_wait
------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait before actually shutting down the resolver thread. This applies
to the cache runtime.

There may still be queries running inside the resolver. Instead of waiting for
all of them to finish, ``vey-proxy`` waits for this fixed interval.

**default**: 30s

.. _conf_resolver_common_protective_query_timeout:

protective_query_timeout
------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for queries sent to the resolver driver. This applies to the cache
runtime.

This value should be greater than any driver-specific timeout.

**default**: 60s

.. _conf_resolver_common_positive_min_ttl:

positive_min_ttl
----------------

**optional**, **type**: u32

Minimum TTL for positive responses. This applies to the resolver driver.

**default**: 30

.. _conf_resolver_common_positive_max_ttl:

positive_max_ttl
----------------

**optional**, **type**: u32

Maximum TTL for positive responses. It should be greater than
``positive_min_ttl``. This applies to the resolver driver.

**default**: 3600

.. _conf_resolver_common_negative_min_ttl:

negative_min_ttl
----------------

**optional**, **type**: u32

Minimum TTL for negative responses. This applies to the resolver driver.

**default**: 30, **alias**: negative_ttl

TTL Calculation
===============

A positive record is cached after it is fetched from the driver. Two TTL values
are then used in the cache runtime:

* expire_ttl

  If the record is still present in cache, it can be returned directly.

  Once this TTL is reached, the next request triggers a fresh query
  immediately.

* vanish_ttl

  The record is removed from the cache.

The cache TTLs are calculated as follows:

.. code-block:: shell

  if [ $RECORD_TTL -gt $(($POSITIVE_MAX_TTL + $POSITIVE_MIN_TTL)) ]
  then
    EXPIRE_TTL=$POSITIVE_MAX_TTL
    VANISH_TTL=$RECORD_TTL
  elif [ $RECORD_TTL -gt $(($POSITIVE_MIN_TTL + $POSITIVE_MIN_TTL)) ]
  then
    EXPIRE_TTL=$(($RECORD_TTL - $POSITIVE_MIN_TTL))
    VANISH_TTL=$RECORD_TTL
  elif [ $RECORD_TTL -gt $POSITIVE_MIN_TTL ]
  then
    EXPIRE_TTL=$POSITIVE_MIN_TTL
    VANISH_TTL=$RECORD_TTL
  else
    EXPIRE_TTL=$POSITIVE_MIN_TTL
    VANISH_TTL=$(($POSITIVE_MIN_TTL + 1))
  fi
