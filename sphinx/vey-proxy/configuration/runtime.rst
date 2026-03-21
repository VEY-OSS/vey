.. _configuration_runtime:

*******
Runtime
*******

The ``runtime`` section is optional. If present, it must be defined in the main
configuration file.

All options in this section are optional and have usable defaults. Change them
only when you understand their impact.

The options are grouped into the following sections:

Tokio Main Runtime
==================

This section describes the main Tokio runtime used by all servers.

thread_number
-------------

**optional**, **type**: int | str

Configures the scheduler type and the number of worker threads.

If the value is ``0``, a basic scheduler is used.
If the value is non-zero, a threaded scheduler is used with the specified
number of worker threads.

**default**: threaded scheduler with one worker thread per available CPU core

thread_name
-----------

**optional**, **type**: str

Sets the name of worker threads. Only ASCII characters are allowed.
Thread-name length may still be limited by the operating system.

**default**: "tokio"

thread_stack_size
-----------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Sets the stack size for worker threads. For plain integer values, the unit is
bytes.

**default**: `tokio thread_stack_size`_

.. _tokio thread_stack_size: https://docs.rs/tokio/0.2.21/tokio/runtime/struct.Builder.html#method.thread_stack_size

max_io_events_per_tick
----------------------

**optional**, **type**: usize

Maximum number of I/O events processed per runtime tick.

**default**: 1024, tokio default value

Daemon Quit Control
===================

This section describes the timing controls used during graceful daemon shutdown.

server_offline_delay
--------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait after receiving a daemon-quit signal before taking all servers
offline.
All listening sockets are closed after this delay, so for graceful restart this
value should be longer than the time required to start the replacement process.

**default**: 4s

.. versionchanged:: 1.7.25 change default value from 4s to 8s

task_wait_delay
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait before checking for live tasks after all servers have entered
offline mode.
Tasks are marked as live only after authentication succeeds, so some delay is
needed to let requests in the negotiation phase either advance or fail.

**default**: 2s

task_wait_timeout
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait before forcefully terminating live tasks after graceful wait
has begun.

**default**: 10h

task_quit_timeout
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

How long to wait before shutting down the process after entering force-quit mode
for all tasks.
Tasks dropped after this timeout will not emit logs.
