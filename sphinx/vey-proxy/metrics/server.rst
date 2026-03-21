.. _metrics_server:

##############
Server Metrics
##############

Server-side metrics describe listener, request, and traffic activity observed on
the client-facing side of ``vey-proxy``.

The following tags are present on all server metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* server

  The server name.

* online

  Whether the server is online. The value is either ``y`` or ``n``.

Listen
======

No additional fixed tags.

The metric names are:

* listen.instance.count

  **type**: gauge

  Number of listening sockets.

* listen.accepted

  **type**: count

  Number of accepted client connections.

* listen.dropped

  **type**: count

  Number of client connections dropped by ACL rules at an early stage.

* listen.timeout

  **type**: count

  Number of client connections that timed out during early protocol negotiation,
  such as TLS setup.

* listen.failed

  **type**: count

  Number of accept errors.

Request
=======

No other fixed tags. Any extra tags configured on the server are also included.

The metric names are:

* server.connection.total

  **type**: count

  Number of accepted client connections.

* server.task.total

  **type**: count

  Number of valid tasks spawned. A client connection becomes a task only after
  negotiation succeeds. User authentication is part of the negotiation stage.

* server.task.alive

  **type**: gauge

  Number of currently running tasks spawned by this server. During a normal
  systemd-managed shutdown, servers with active tasks enter offline mode and
  wait for those tasks to complete.

Forbidden
=========

No other fixed tags. Any extra tags configured on the server are also included.

The metric names are:

* server.forbidden.auth_failed

  **type**: count

  Number of requests rejected because authentication failed, for example because
  no user was supplied or the user token did not match.

* server.forbidden.dest_denied

  **type**: count

  Number of requests rejected because the destination was denied.

  This metric is also added to user forbidden metrics when possible.

  .. note:: Only denials caused by server-level rules are counted here.

* server.forbidden.user_blocked

  **type**: count

  Number of requests received from blocked users.

* server.forbidden.invalid_param

  **type**: count

  Number of requests that contained invalid username parameters.

  .. versionadded:: 1.13.0

Traffic
=======

The following tag is also set:

* :ref:`transport <metrics_tag_transport>`

Any extra tags configured on the server are also included.

These I/O metrics count application-layer traffic only. Lower-layer overhead,
such as TLS framing, is not included.

The metric names are:

* server.traffic.in.bytes

  **type**: count

  Total bytes received from the client.

* server.traffic.in.packets

  **type**: count

  Total datagram packets received from the client.
  This metric is not available for stream-oriented transports.

* server.traffic.out.bytes

  **type**: count

  Total bytes sent to the client.

* server.traffic.out.packets

  **type**: count

  Total datagram packets sent to the client.
  This metric is not available for stream-oriented transports.

Untrusted
=========

An untrusted task is a task that is invalid but must still be drained safely.

No other fixed tags. Any extra tags configured on the server are also included.

The metric names are:

* server.task.untrusted_total

  **type**: count

  Number of untrusted tasks spawned.

* server.task.untrusted_alive

  **type**: gauge

  Number of currently running untrusted tasks spawned by this server.

* server.traffic.untrusted_in.bytes

  **type**: count

  Total bytes received from the client in untrusted requests.
