.. _log_task:

********
Task Log
********

Each valid request becomes a task. Every task generates a log record when it
finishes, and may generate intermediate records depending on configuration and
event type.

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

The task log subtype. The meaning of non-shared keys depends on this value.

task_id
-------

**required**, **type**: uuid in simple string format

The UUID of the task.

The same ``task_id`` also appears in related logs such as escape logs.

task_event
----------

**optional**, **type**: string

The event that triggered this log entry.

Possible values are:

  - Created: task created
  - Connected: connected to upstream
  - Periodic: periodic log
  - ClientShutdown: client shutdown the connection gracefully first
  - UpstreamShutdown: upstream shutdown the connection gracefully first
  - Finished: task finished

This field may be omitted when the value is ``Finished``.

.. versionadded:: 1.11.0

stage
-----

**required**, **type**: enum string

The current stage of the task.

The values available for a task depend on the server protocol. The full set of
possible stage values is:

* Created

  The task has just been created.

* Preparing

  Internal resources are being prepared.

* Connecting

  ``vey-proxy`` is trying to connect to the remote peer.

* Connected

  The remote peer connection has just been established.

* Replying

  ``vey-proxy`` is replying to the client that the remote peer connection is
  ready.

* LoggedIn

  The upstream required login and login has completed.

* Relaying

  Both client-side and remote-side channels are established and data is being
  relayed.

* Finished

  The task finished successfully. This stage is available only for layer-7
  protocols.

start_at
--------

**required**, **type**: rfc3339 timestamp string with microseconds

The time at which the task was created, after validation.

.. note:: Not every request will be a task, only the valid ones.

user
----

**optional**, **type**: string

The username. Present only when user authentication is enabled on the server.

escaper
-------

**optional**, **type**: string

The selected escaper name.

reason
------

**optional**, **type**: enum string

The short reason why the task ended.

See the definition of **ServerTaskError** in code file *src/serve/error.rs*.

wait_time
---------

**optional**, **type**: time duration string

The time spent between accepting the request and creating the task.

For requests that reuse an existing connection, the start time is when
``vey-proxy`` begins polling for the next request. As a result, ``wait_time``
may be unexpectedly large in some logs. This behavior may change in the future.

.. _log_task_ready_time:

ready_time
----------

**optional**, **type**: time duration string

The time spent between task creation and the relaying stage, which means both
the client-side and remote-side channels have been established. This value may
be absent if the task failed early.

total_time
----------

**optional**, **type**: time duration string

The time between task creation and the emission of this log record.

Sub Types
=========

.. toctree::
   :maxdepth: 1

   tcp_connect
   http_forward
   ftp_over_http
   udp_associate
   udp_connect
