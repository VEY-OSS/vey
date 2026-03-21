.. _configure_runtime_value_types:

*******
Runtime
*******

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: available
   - ``vey-statsd``: not currently used

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

.. _conf_value_cpu_set:

cpu set
=======

**yaml value** seq | str | usize

``CPU_SET(3)`` value for use with ``sched_setaffinity(2)``.

The value can be a single CPU ID or a sequence of CPU IDs.

Each CPU ID may be expressed as:

 - usize: a single CPU ID
 - string: :ref:`cpu id list str <conf_value_cpu_id_list_str>`

.. _CPU_SET(3): https://man7.org/linux/man-pages/man3/CPU_SET.3.html
.. _sched_setaffinity(2): https://man7.org/linux/man-pages/man2/sched_setaffinity.2.html

.. availability::


   - ``vey-proxy``: changed in ``1.11.3``: allow a list of CPU ID string values

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

**default**: the number of logic CPU cores, **alias**: threads_total, thread_number_total

thread_number_per_runtime
-------------------------

**optional**, **type**: non-zero usize

Number of threads used by each Tokio runtime.

**default**: 1, **alias**: threads_per_runtime

.. availability::


   - ``vey-proxy``: available since ``1.11.3``

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

  - otherwise if thread_number_per_runtime is set to 1

    a default CPU SET will be set for each thread, the CPU ID in the set will match the thread ID.

* if false, no CPU affinity is set, the same as omitting this option.

**default**: no sched affinity set

max_io_events_per_tick
----------------------

**optional**, **type**: usize

Maximum number of I/O events processed per runtime tick.

**default**: 1024, tokio default value
