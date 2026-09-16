.. _log_task_h2_connection:

*************
H2 Connection
*************

``H2Connection`` task logs are emitted by
:ref:`http_guard <configuration_server_http_guard>` for the client HTTP/2
connection that accepts streams. Each such connection has one
``H2Connection`` task. Streams on that connection log as
:ref:`H2Forward <log_task_h2_forward>` or :ref:`Websocket <log_task_websocket>`
and carry the same ``connection_id`` as this task's ``task_id``.

``task_type`` is ``H2Connection``.

These logs emit:

* ``Created``, when :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
  is enabled
* ``Periodic``, when :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
  is set
* ``Finished``, when the connection ends

They do **not** emit ``Connected``, ``ClientShutdown``, or
``UpstreamShutdown``. There is no ``reason`` key: the slog record message is
``finished`` on success, or the error string on failure.

This task has no request ``stage`` and no ``ready_time``. Shared keys still
apply for ``server_type``, ``server_name``, ``task_id``, ``start_at``,
``wait_time``, and ``total_time``. ``user`` is the visitor username when
visitor authentication is enabled. ``site`` and ``tenant`` are reverse-proxy
keys: the site id, and the site owner username when the site has an
:ref:`owner <conf_site_owner>`.

There is no origin hop on this task: ``upstream``, ``escaper``, and the
``next_*`` / TCP-connect keys are not present. Origin I/O is recorded on the
stream tasks. Connection-level HTTP/2 I/O uses the client TCP copy counters
below.

.. versionadded:: 1.15.0

server_addr
-----------

**required**, **type**: socket address string

The listening address of the server.

client_addr
-----------

**required**, **type**: socket address string

The client address.

connection_id
-------------

**required**, **type**: uuid in simple string format

The HTTP/2 connection id. Equal to ``task_id`` on this record. Stream tasks
on this connection log the same value.

first_stream_at
---------------

**optional**, **type**: rfc3339 timestamp string with microseconds

The time at which the first client stream was accepted on this connection.

Present on ``Periodic`` and ``Finished`` records after at least one stream
has been accepted.

stream_total
------------

**optional**, **type**: int

Number of client streams accepted on this connection (H2Forward and
Websocket).

Present on ``Periodic`` and ``Finished`` records.

stream_alive
------------

**optional**, **type**: int

Number of client streams still running on this connection.

Present on ``Periodic`` and ``Finished`` records.

c_rd_bytes
----------

**optional**, **type**: int

Total bytes received from the client on this HTTP/2 connection.

Present on ``Periodic`` and ``Finished`` records.

c_wr_bytes
----------

**optional**, **type**: int

Total bytes sent to the client on this HTTP/2 connection.

Present on ``Periodic`` and ``Finished`` records.
