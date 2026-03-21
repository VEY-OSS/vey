.. _log_request:

***********
Request Log
***********

A request log is generated when an individual keyless request fails.

The following keys are present in a request log.

server_name
-----------

**required**, **type**: string

Name of the server that accepted the request.

task_id
-------

**required**, **type**: uuid in simple string format

Task UUID.

msg_id
------

**required**, **type**: usize string

Message ID field from the request.

create_at
---------

**required**, **type**: rfc3339 timestamp string with microseconds

Creation timestamp of the request.

.. versionadded:: 0.4.2

.. _log_request_process_time:

process_time
------------

**required**, **type**: time duration string

Time spent processing this request.

.. versionadded:: 0.4.2
