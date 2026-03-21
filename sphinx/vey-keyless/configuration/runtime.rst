.. _configuration_runtime:

*******
Runtime
*******

This is the ``runtime`` configuration. It is optional and must reside in the
main configuration file if used.

All options in this section are optional and have reasonable defaults.
Set them only when you need to tune runtime behavior explicitly.

The options can be grouped into the following sections:

tokio main runtime
==================

This section describes the main Tokio runtime used by all servers.

thread_number
-------------

**optional**, **type**: int | str

Scheduler mode and worker-thread count.

If set to ``0``, a basic scheduler is used.
Otherwise, a threaded scheduler is used with the specified worker-thread count.

**default**: threaded scheduler with worker threads on all available CPU cores

thread_name
-----------

**optional**, **type**: str

Name used for spawned worker threads. Only ASCII characters are allowed.
The operating system may impose its own length limit.

**default**: "tokio"

thread_stack_size
-----------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Stack size for worker threads. Plain integer values are interpreted as bytes.

**default**: `tokio thread_stack_size`_

.. _tokio thread_stack_size: https://docs.rs/tokio/0.2.21/tokio/runtime/struct.Builder.html#method.thread_stack_size

max_io_events_per_tick
----------------------

**optional**, **type**: usize

Configures the max number of events to be processed per tick.

**default**: 1024, tokio default value

daemon quit control
===================

This section describes graceful-shutdown behavior for the daemon.

server_offline_delay
--------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Delay before all servers are taken offline after the daemon receives a quit
signal. All listening sockets are closed after this interval, so it should be
longer than the time needed to start a replacement daemon if you depend on
graceful restart.

**default**: 4s

task_wait_delay
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Delay before checking live tasks after all servers enter offline mode.
Tasks are marked alive only after successful negotiation, so this gives tasks
still in setup time to reach a stable state.

**default**: 2s

task_wait_timeout
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Maximum time to wait for live tasks to finish gracefully before forcing them to quit.

**default**: 10h

task_quit_timeout
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Delay before the process shuts down after entering forced-quit mode for all
tasks. Tasks dropped after this timeout do not emit logs.
