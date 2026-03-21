.. _metrics_runtime:

###############
Runtime Metrics
###############

These metrics are emitted by runtimes that expose internal scheduler state.

The following tags are present on all runtime metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* runtime_id

  The runtime ID or label.

  There may be multiple instances of the same runtime type. This tag
  distinguishes them.

.. _metrics_runtime_tokio:

Tokio Runtime Metrics
=====================

These metrics come from the Tokio runtime.

* runtime.tokio.alive_tasks

  **type**: gauge

  The current number of live tasks in the runtime.

* runtime.tokio.global_queue_depth

  **type**: gauge

  The number of tasks currently scheduled in the runtime's global queue.
