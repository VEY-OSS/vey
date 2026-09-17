.. _log_task_h2_stream:

*********
H2 Stream
*********

``H2Stream`` task logs are emitted by
:ref:`http_guard <configuration_server_http_guard>` when an HTTP/2 connection
accepts a client stream. The task only classifies the stream: it either
rejects it, or creates a :ref:`H2Forward <log_task_h2_forward>` or
:ref:`Websocket <log_task_websocket>` task and hands the stream over.

``task_type`` is ``H2Stream``.

These logs emit only ``Finished``. They do **not** emit ``Created``,
``Connected``, ``Periodic``, ``ClientShutdown``, or ``UpstreamShutdown``.
There is no ``reason`` key and no request ``stage``. The slog record message
is the processing result: ``H2Forward`` or ``Websocket`` when a new task was
created, or an error string when the stream was rejected.

Shared keys still apply for ``server_type``, ``server_name``, ``task_id``,
``start_at``, ``wait_time``, and ``total_time``. ``user`` is the visitor
username when visitor authentication is enabled. ``site`` and ``tenant`` are
reverse-proxy keys: the site id, and the site owner username when the site
has an :ref:`owner <conf_site_owner>`.

There is no origin hop on this task: ``upstream``, ``escaper``, and the
``next_*`` / TCP-connect keys are not present. Those belong to the spawned
forward or websocket task.

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

The parent HTTP/2 connection id. Equal to ``task_id`` /
``connection_id`` on the :ref:`H2Connection <log_task_h2_connection>`
record.

clt_stream
----------

**required**, **type**: int

The client HTTP/2 stream id.

next_task_type
--------------

**optional**, **type**: enum string

The task type created from this stream: ``H2Forward`` or ``Websocket``.

Present only when classification succeeded.

next_task_id
------------

**optional**, **type**: uuid in simple string format

The ``task_id`` of the created :ref:`H2Forward <log_task_h2_forward>` or
:ref:`Websocket <log_task_websocket>` task.

Present only when classification succeeded.

rsp_status
----------

**optional**, **type**: int

The status code of the locally generated error response.

Present only when the stream was rejected.
