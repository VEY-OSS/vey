.. _metrics_runtime:

###############
Runtime Metrics
###############

Runtime metrics are available for runtime implementations that expose metric reporting.

The following tags are present on all runtime metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`

The following tags are present on all tokio runtime metrics:

* :ref:`stat_id <metrics_tag_stat_id>`

* runtime_id

  The runtime ID or label.

  There may be multiple instances of the same runtime type. This tag
  distinguishes between them.

.. _metrics_runtime_jemalloc:

Jemalloc Runtime Metrics
========================

There metrics come from the jemalloc memory allocator.

* runtime.jemalloc.approximate_active

  **type**: gauge

  The total number of bytes in active pages collected in an unsynchronized manner, without requiring an epoch update.

  .. versionadded:: 0.5.0

.. _metrics_runtime_tokio:

Tokio Runtime Metrics
=====================

These metrics come from the Tokio runtime.

The supported metrics are:

* runtime.tokio.alive_tasks

  **type**: gauge

  Current number of live tasks in the runtime.

* runtime.tokio.global_queue_depth

  **type**: gauge

  Number of tasks currently scheduled in the runtime's global queue.
