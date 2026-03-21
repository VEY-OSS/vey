.. _configuration_runtime:

*******
Runtime
*******

This is the optional ``runtime`` section. If present, it must be defined in
the main configuration file.

All options in this section have sensible defaults. Override them only when
you need to tune runtime behavior explicitly.

The options can be grouped into the following sections:

tokio main runtime
==================

This section describes the main Tokio runtime used by all servers.

thread_number
-------------

**optional**, **type**: int | str

Set the scheduler mode and number of worker threads.

If set to ``0``, a current-thread runtime is used.
If set to a non-zero value, a multi-threaded runtime is used with the
specified worker-thread count.

**default**: a multi-threaded runtime with one worker thread per CPU core

thread_name
-----------

**optional**, **type**: str

Set the name prefix for spawned worker threads. Only ASCII characters are
allowed. The effective length may still be limited by the OS.

**default**: "tokio"

thread_stack_size
-----------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Set the stack size for worker threads. For *<int>* value, the unit is bytes.

**default**: `tokio thread_stack_size`_

.. _tokio thread_stack_size: https://docs.rs/tokio/0.2.21/tokio/runtime/struct.Builder.html#method.thread_stack_size

max_io_events_per_tick
----------------------

**optional**, **type**: usize

Set the maximum number of I/O events processed in a single runtime tick.

**default**: 1024, tokio default value

daemon quit control
===================

This section describes the timers used during graceful shutdown.

server_offline_delay
--------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the delay before taking all servers offline after the daemon receives a
shutdown signal.

All listening sockets are closed after this delay. For graceful restarts, it
should be longer than the time needed to start the replacement process.

**default**: 4s

task_wait_delay
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the delay before checking live tasks after all servers enter offline mode.

Some tasks may still be in negotiation when shutdown begins, so this delay
allows them to transition into a stable state before the daemon decides
whether to wait for them.

**default**: 2s

task_wait_timeout
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set how long to wait for live tasks to exit gracefully before forcing them to
quit.

**default**: 10h

task_quit_timeout
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set how long the process stays alive after it enters forced-quit mode for all
tasks. Tasks dropped after this timeout will not emit final logs.
