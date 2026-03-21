.. _log_task:

********
Task Log
********

Each accepted request becomes a task. Every task emits a final log record, and
some task types can also emit lifecycle logs while running.

Shared Keys
===========

The following shared keys are present in all task logs:

server_type
-----------

**required**, **type**: enum string

The type of the server that accepted the request.

server_name
-----------

**required**, **type**: string

The name of the server that accepted the request.

task_type
---------

**required**, **type**: enum string

The task-log subtype. The meaning of non-shared keys depends on this value.

task_id
-------

**required**, **type**: uuid in simple string format

The task UUID, in simple-string format.

If other logs are associated with this task, they use the same ``task_id``.

task_event
----------

**optional**, **type**: string

The event that triggered this log record.

The event can be

  - Created: the task was created
  - Connected: the upstream connection was established
  - Periodic: the task emitted a periodic state log
  - ClientShutdown: the client closed the connection first
  - UpstreamShutdown: the upstream peer closed the connection first
  - Finished: the task finished

This field may be omitted when the event is ``Finished``.

.. versionadded:: 0.3.8

stage
-----

**required**, **type**: enum string

The current task stage.

The available values depend on the server protocol. The full set is:

* Created

  The task has just been created.

* Preparing

  Internal resources are being prepared.

* Connecting

  The daemon is connecting to the remote peer.

* Connected

  The remote peer has just been connected.

* Replying

  The daemon is replying to the client that the upstream connection is ready.

* LoggedIn

  The upstream requires login and authentication has completed.

* Relaying

  Both client and upstream channels are established and data is being relayed.

* Finished

  The task has finished without error. This stage is used only by layer-7
  protocols.

start_at
--------

**required**, **type**: rfc3339 timestamp string with microseconds

The time when the task is created, after request validation succeeds.

.. note:: Only valid requests are promoted to tasks.

reason
------

**required**, **type**: enum string

The short reason why the task ended.

See the ``ServerTaskError`` definition in ``src/serve/error.rs`` for the full
reason set.

wait_time
---------

**optional**, **type**: time duration string

How long it took from request acceptance to task creation.

For requests on reused connections, the start time is the moment the daemon
begins polling for the next request. That can produce a large ``wait_time``.

.. _log_task_ready_time:

ready_time
----------

**optional**, **type**: time duration string

How long it took from task creation to the relaying stage, when both the
client channel and remote channel are established. The value may be empty if
the task fails early.

total_time
----------

**required**, **type**: time duration string

How long the task ran from creation to completion.

Sub Types
=========

.. toctree::
   :maxdepth: 1

   tcp_connect
   keyless
