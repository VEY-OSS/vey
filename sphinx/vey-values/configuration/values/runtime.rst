.. _configure_runtime_value_types:

*******
Runtime
*******

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: available
   - ``vey-statsd``: available

.. _conf_value_cpu_id_list_str:

cpu id list str
===============

A string that represents a list of CPU IDs.

Supported forms include:

 - A single CPU ID
 - CPU ID range in the form `<start>-<end>`, where `start` should be less than `end`.
 - A list of CPU ID / CPU ID range delimited by ','

.. availability::


   - ``vey-proxy``: available since ``1.11.3``
   - ``vey-statsd``: available

.. _conf_value_cpu_set:

cpu set
=======

**yaml value**: seq | str | usize

``CPU_SET(3)`` value for use with ``sched_setaffinity(2)``.

The value can be a single CPU ID or a sequence of CPU IDs.

Each CPU ID may be expressed as:

 - usize: a single CPU ID
 - string: :ref:`cpu id list str <conf_value_cpu_id_list_str>`

Examples:

.. code-block:: yaml

   sched_affinity:
     0: "0-3"
     1:
       - 4
       - "5-7"

.. _CPU_SET(3): https://man7.org/linux/man-pages/man3/CPU_SET.3.html
.. _sched_setaffinity(2): https://man7.org/linux/man-pages/man2/sched_setaffinity.2.html

.. availability::


   - ``vey-proxy``: changed in ``1.11.3``: allow a list of CPU ID string values
   - ``vey-statsd``: available

.. _conf_value_daemon_runtime_config:

daemon runtime config
=====================

**yaml value**: map

Configuration for the main daemon runtime.

The supported keys are grouped into the following sections.

Tokio Main Runtime
------------------

thread_number
^^^^^^^^^^^^^

**optional**, **type**: int | str

Configures the scheduler mode and worker-thread count.

If set to ``0``, a current-thread runtime is used.
If set to a non-zero value, a multi-threaded runtime is used with the
specified worker-thread count.

**default**: a multi-threaded runtime with one worker thread per available CPU
core

thread_name
^^^^^^^^^^^

**optional**, **type**: str

Sets the name prefix used for spawned worker threads. Only ASCII characters are
allowed. The effective length may still be limited by the operating system.

**default**: ``tokio``

thread_stack_size
^^^^^^^^^^^^^^^^^

**optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

Sets the stack size for worker threads. Plain integer values are interpreted as
bytes.

**default**: `tokio thread_stack_size`_

.. _tokio thread_stack_size: https://docs.rs/tokio/0.2.21/tokio/runtime/struct.Builder.html#method.thread_stack_size

max_io_events_per_tick
^^^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: usize

Maximum number of I/O events processed per runtime tick.

**default**: ``1024``, Tokio default value

Daemon Quit Control
-------------------

server_offline_delay
^^^^^^^^^^^^^^^^^^^^

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

How long to wait after receiving a daemon-quit signal before taking all servers
offline.

All listening sockets are closed after this delay, so for graceful restart this
value should be longer than the time needed to start the replacement process.

**default**: ``4s``

.. availability::


   - ``vey-proxy``: available

task_wait_delay
^^^^^^^^^^^^^^^

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

How long to wait before checking live tasks after all servers have entered
offline mode.

Some tasks may still be in negotiation when shutdown begins, so this delay lets
them reach a stable state before the daemon decides whether to wait for them.

**default**: ``2s``

task_wait_timeout
^^^^^^^^^^^^^^^^^

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

How long to wait for live tasks to exit gracefully before forcing them to quit.

**default**: ``10h``

task_quit_timeout
^^^^^^^^^^^^^^^^^

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

How long the process stays alive after it enters forced-quit mode for all
tasks. Tasks dropped after this timeout will not emit final logs.

**default**: ``30min``

.. _conf_value_unaided_runtime_config:

unaided runtime config
======================

**yaml value**: map

Configuration for an unaided runtime.

The supported keys are:

thread_number
-------------

**optional**, **type**: non-zero usize

Total thread count.

**default**: the number of logical CPU cores, **alias**: threads_total, thread_number_total

thread_number_per_runtime
-------------------------

**optional**, **type**: non-zero usize

Number of threads used by each Tokio runtime.

**default**: 1, **alias**: threads_per_runtime

.. availability::


   - ``vey-proxy``: available since ``1.11.3``
   - ``vey-statsd``: available

thread_stack_size
-----------------

**optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

Stack size for worker threads. Plain integer values are interpreted as bytes.

**default**: system default

sched_affinity
--------------

**optional**, **type**: map | bool

CPU affinity configuration for worker threads.

For map value, the key should be the thread id starting from 0, and the value should be :ref:`cpu set <conf_value_cpu_set>`.

For bool value:

* if true

  - if found any `WORKER_<N>_CPU_LIST` environment variables

    it will set the CPU affinity for that corresponding runtime `<N>`, the value should be :ref:`cpu id list str <conf_value_cpu_id_list_str>`.

    .. availability::


       - ``vey-proxy``: available since ``1.11.3``
       - ``vey-statsd``: available

  - otherwise if thread_number_per_runtime is set to 1

    a default CPU SET will be set for each thread, the CPU ID in the set will match the thread ID.

* if false, no CPU affinity is set, the same as omitting this option.

**default**: no sched affinity set

The loader also supports automatic affinity assignment when
``sched_affinity: true``. In that mode it first checks ``WORKER_<N>_CPU_LIST``
environment variables and otherwise falls back to automatic one-thread-per-CPU
mapping when ``thread_number_per_runtime`` is ``1``.

max_io_events_per_tick
----------------------

**optional**, **type**: usize

Maximum number of I/O events processed per runtime tick.

**default**: 1024, tokio default value
