.. _metrics_runtime:

###############
Runtime Metrics
###############

Runtime metrics are available for runtime implementations that expose metric
reporting. Allocator metrics are emitted only when the binary is built with
that allocator.

The following tag is present on all runtime metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`

The following tags are also present on Tokio runtime metrics:

* :ref:`stat_id <metrics_tag_stat_id>`

* runtime_id

  The runtime ID or label.

  There may be multiple instances of the same runtime type. This tag
  distinguishes between them.

.. _metrics_runtime_jemalloc:

Jemalloc Runtime Metrics
========================

These metrics come from the jemalloc memory allocator.

* runtime.jemalloc.approximate_active

  **type**: gauge

  The total number of bytes in active pages collected in an unsynchronized
  manner, without requiring an epoch update.

.. _metrics_runtime_mimalloc:

Mimalloc Runtime Metrics
========================

These metrics come from the mimalloc memory allocator.

* runtime.mimalloc.current_pages

  **type**: gauge

  Current count of mimalloc pages.

* runtime.mimalloc.peak_pages

  **type**: gauge

  Peak count of mimalloc pages.

* runtime.mimalloc.current_commit

  **type**: gauge

  Current committed memory (backed by the page file).

* runtime.mimalloc.peak_commit

  **type**: gauge

  Peak committed memory (backed by the page file).

.. _metrics_runtime_snmalloc:

snmalloc Runtime Metrics
========================

These metrics come from the snmalloc memory allocator.

* runtime.snmalloc.current_memory_usage

  **type**: gauge

  Bytes currently reserved from the OS.

* runtime.snmalloc.peak_memory_usage

  **type**: gauge

  High-water mark of current memory usage.

.. _metrics_runtime_tokio:

Tokio Runtime Metrics
=====================

These metrics come from the Tokio runtime.

* runtime.tokio.alive_tasks

  **type**: gauge

  Current number of live tasks in the runtime.

* runtime.tokio.global_queue_depth

  **type**: gauge

  Number of tasks currently scheduled in the runtime's global queue.
