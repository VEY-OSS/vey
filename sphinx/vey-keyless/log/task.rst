.. _log_task:

********
Task Log
********

Each client connection is handled as a task. A task log is emitted when the
connection finishes.

The following keys are present in a task log.

server_name
-----------

**required**, **type**: string

Name of the server that accepted the connection.

task_id
-------

**required**, **type**: uuid in simple string format

Task UUID.

The ``task_id`` also appears in related logs, such as request logs associated
with this task.

server_addr
-----------

**required**, **type**: socket address string

Listening address of the server.

client_addr
-----------

**required**, **type**: socket address string

Client address.

start_at
--------

**required**, **type**: rfc3339 timestamp string with microseconds

Time when the task was created, after validation.

.. note:: Not every request will be a task, only the valid ones.
