.. _metrics_resolver:

################
Resolver Metrics
################

Resolver metrics describe DNS query activity and resolver cache state.

The following tags are present on all resolver metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* resolver

  The resolver name.

* rr_type

  The queried RR type, such as ``A`` or ``AAAA``.

Query
=====

The metric names are:

* resolver.query.total

  **type**: count

  Total queries handled by this resolver.

* resolver.query.cached

  **type**: count

  Total queries answered from the local cache.

* resolver.query.trashed

  **type**: count

  Total queries answered from the local trash cache.

  .. versionadded:: 1.11.6

* resolver.query.driver.total

  **type**: count

  Total queries that triggered an actual upstream DNS lookup.

* resolver.query.driver.timeout

  **type**: count

  Total upstream DNS queries that timed out.

* resolver.query.driver.failed

  **type**: count

  Total upstream DNS queries failed when processing by the resolve driver.

  .. versionadded:: 1.13.2

* resolver.query.server.refused

  **type**: count

  Total queries for which the DNS server returned ``REFUSED``.

* resolver.query.server.malformed

  **type**: count

  Total queries for which the DNS server returned ``FormErr``.

* resolver.query.server.not_found

  **type**: count

  Total queries for which the DNS server returned ``NXDOMAIN`` or an equivalent
  not-found response.

* resolver.query.server.serv_fail

  **type**: count

  Total queries for which the DNS server returned ``SERVFAIL``.

* resolver.query.server.other_code

  **type**: count

  Total queries for which the DNS server returned other response code.

  .. versionadded:: 1.13.2

Memory
======

The metric names are:

* resolver.memory.cache.capacity

  **type**: gauge

  Capacity of the result cache hash table.

* resolver.memory.cache.length

  **type**: gauge

  Number of records currently stored in the result cache hash table.

* resolver.memory.doing.capacity

  **type**: gauge

  Capacity of the in-flight query hash table.

* resolver.memory.doing.length

  **type**: gauge

  Number of records currently stored in the in-flight query hash table.

* resolver.memory.trash.capacity

  **type**: gauge

  Capacity of the result trash hash table.

  .. versionadded:: 1.11.6

* resolver.memory.trash.length

  **type**: gauge

  Number of records currently stored in the result trash hash table.

  .. versionadded:: 1.11.6
