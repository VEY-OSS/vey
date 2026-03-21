.. _metrics_server:

##############
Server Metrics
##############

Server-side metrics describe listener activity, task creation, and traffic
handled for clients.

The following tags are present on all server metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* server

  The server name.

* online

  Whether the server is online. The value is either ``y`` or ``n``.

Listen
======

No extra tags.

The metric names are:

* listen.instance.count

  **type**: gauge

  The number of listening sockets.

* listen.accepted

  **type**: count

  The number of client connections accepted.

* listen.dropped

  **type**: count

  The number of client connections dropped early by ACL rules.

* listen.timeout

  **type**: count

  The number of client connections that timed out during early protocol
  negotiation, such as TLS handshake setup.

* listen.failed

  **type**: count

  The number of accept errors.

Request
=======

No additional fixed tags are defined. Extra server-level tags are appended if
configured.

The metric names are:

* server.connection.total

  **type**: count

  The number of accepted client connections.

* server.task.total

  **type**: count

  The number of valid tasks spawned. A client connection becomes a task only
  after negotiation succeeds. Authentication is also part of this stage.

* server.task.alive

  **type**: gauge

  The number of live tasks currently running for this server. During a normal
  shutdown, servers with active tasks go offline first and then wait for them
  to finish.

Traffic
=======

The following additional tag is present:

* :ref:`transport <metrics_tag_transport>`

Extra server-level tags are appended if configured.

These I/O counters cover only application-layer traffic. Lower-layer traffic,
such as TLS framing overhead, is not included.

The metric names are:

* server.traffic.in.bytes

  **type**: count

  The total number of bytes received from clients.

* server.traffic.in.packets

  **type**: count

  The total number of datagram packets received from clients.
  This metric is not available for stream-oriented transports.

* server.traffic.out.bytes

  **type**: count

  The total number of bytes sent to clients.

* server.traffic.out.packets

  **type**: count

  The total number of datagram packets sent to clients.
  This metric is not available for stream-oriented transports.
