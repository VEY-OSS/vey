.. _metrics_user:

############
User Metrics
############

User metrics describe per-user application-layer activity. They can be grouped
into request and traffic metrics.

The following tags are present on all user metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* user_group

  The user group name.

* user

  The user name.

.. _metrics_user_user_type:

* user_type

  The user type.

  Current supported values are:

    - Static
    - Dynamic

Request
=======

The following tags are set on metrics in this section:

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

The following tag is also set on ``user.connection.*`` metrics:

* :ref:`connection <metrics_tag_connection>`

The following tag is also set on ``user.request.*`` metrics:

* :ref:`request <metrics_tag_request>`

The metric names are:

* user.connection.total

  **type**: count

  Number of client connections from the user. Connections that fail during
  authentication are not counted.

* user.forbidden.crypto_error

  **type**: count

  Number of rejected requests caused by internal crypto error.

  .. versionadded:: 1.13.2

* user.forbidden.auth_failed

  **type**: count

  Number of rejected requests caused by authentication failure, such as a token
  mismatch.

* user.forbidden.user_expired

  **type**: count

  Number of rejected requests caused by the user expiring while the request was
  being handled.

* user.forbidden.user_blocked

  **type**: count

  Number of rejected requests caused by the user being blocked while the
  request was being handled.

* user.forbidden.fully_loaded

  **type**: count

  Number of requests rejected because the maximum number of active requests was
  reached.

* user.forbidden.rate_limited

  **type**: count

  Number of requests rejected because the user's rate limit was exceeded.

* user.forbidden.proto_banned

  **type**: count

  Number of requests rejected because the proxy request type was banned.

* user.forbidden.dest_denied

  **type**: count

  Number of requests rejected because the destination was forbidden.

  Denials caused by server-level rules are also counted here.

* user.forbidden.ip_blocked

  **type**: count

  Number of requests rejected because the resolved IP address was blocked.

  Denials caused by escaper-level rules are also counted here.

* user.forbidden.log_skipped

  **type**: count

  Number of requests for which logging was skipped.

* user.forbidden.ua_blocked

  **type**: count

  Number of layer-7 HTTP requests blocked by User-Agent matching.

* user.request.total

  **type**: count

  Total requests received from the user. This value may be greater than
  ``user.connection.total`` because some protocols can reuse a connection for
  multiple requests.

* user.request.alive

  **type**: gauge

  Number of currently active requests for the user.

* user.request.ready

  **type**: count

  Total tasks that reached the *ready* stage for the user. The remote
  connection may be a new connection or a reused keepalive connection.

* user.request.reuse

  **type**: count

  Total number of attempts to reuse an existing remote keepalive connection.
  Reuse attempts may still fail.

* user.request.renew

  **type**: count

  Total number of failed attempts to reuse an existing remote keepalive
  connection. After a recoverable reuse failure, a new connection is created and
  the request is retried.

* user.l7.connection.alive

  **type**: gauge

  Number of currently active layer-7 proxy connections.

Traffic
=======

The following tags are set on metrics in this section:

* :ref:`request <metrics_tag_request>`

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

These I/O metrics include application-layer traffic only. SOCKS negotiation
data and HTTPS-forward TLS overhead are not included.

The metric names are:

* user.traffic.in.bytes

  **type**: count

  Total bytes received from the client.

* user.traffic.in.packets

  **type**: count

  Total datagram packets received from the client.
  This metric is not available for stream-oriented transports.

* user.traffic.out.bytes

  **type**: count

  Total bytes sent to the client.

* user.traffic.out.packets

  **type**: count

  Total datagram packets sent to the client.
  This metric is not available for stream-oriented transports.

Upstream Traffic
================

The following tags are set on metrics in this section:

* :ref:`transport <metrics_tag_transport>`

* escaper

  The escaper name that handled the upstream side of the request.

Any extra tags configured on the escaper are also included.

These I/O metrics include application-layer traffic only. HTTPS-forward TLS
overhead is not included.

The metric names are:

* user.upstream.traffic.in.bytes

  **type**: count

  Total bytes received from the upstream side.

* user.upstream.traffic.in.packets

  **type**: count

  Total datagram packets received from the upstream side.
  This metric is not available for stream-oriented transports.

* user.upstream.traffic.out.bytes

  **type**: count

  Total bytes sent to the upstream side.

* user.upstream.traffic.out.packets

  **type**: count

  Total datagram packets sent to the upstream side.
  This metric is not available for stream-oriented transports.
