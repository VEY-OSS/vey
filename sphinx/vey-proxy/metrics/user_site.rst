.. _metrics_user_site:

#################
User Site Metrics
#################

User-site metrics describe application-layer activity for each explicit site
defined under a user.

Metric names use the prefix ``user.<site_id>`` where *site_id* is the value of
the :ref:`id <conf_auth_user_site_id>` configuration option.

The following tags are present on all user-site metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* user_group

  The user group name.

* user

  The user name.

* user_type

  The user type. See :ref:`user type <metrics_user_user_type>` for details.

Request
=======

The following tags are set on metrics in this section:

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

The following tag is also set on ``user.<site_id>.connection.*`` metrics:

* :ref:`connection <metrics_tag_connection>`

The following tag is also set on ``user.<site_id>.request.*`` metrics:

* :ref:`request <metrics_tag_request>`

The metric names are:

* user.<site_id>.connection.total

  **type**: count

  Number of client connections from the user for this site. Connections that
  fail during authentication are not counted.

* user.<site_id>.request.total

  **type**: count

  Total requests received from the user for this site. This value may be
  greater than ``user.<site_id>.connection.total`` because some protocols can
  reuse a connection for multiple requests.

* user.<site_id>.request.alive

  **type**: gauge

  Number of currently active requests for this site.

* user.<site_id>.request.ready

  **type**: count

  Total tasks for this site that reached the *ready* stage. The remote
  connection may be new or a reused keepalive connection.

* user.<site_id>.request.reuse

  **type**: count

  Total attempts to reuse an existing remote keepalive connection.
  Reuse attempts may still fail.

* user.<site_id>.request.renew

  **type**: count

  Total failed attempts to reuse an existing remote keepalive connection. After
  a recoverable reuse failure, a new connection is created and the request is
  retried.

* user.<site_id>.l7.connection.alive

  **type**: gauge

  Number of currently active layer-7 proxy connections for this site.

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

* user.<site_id>.traffic.in.bytes

  **type**: count

  Total bytes received from the client.

* user.<site_id>.traffic.in.packets

  **type**: count

  Total datagram packets received from the client.
  This metric is not available for stream-oriented transports.

* user.<site_id>.traffic.out.bytes

  **type**: count

  Total bytes sent to the client.

* user.<site_id>.traffic.out.packets

  **type**: count

  Total datagram packets sent to the client.
  This metric is not available for stream-oriented transports.

Duration
========

The following tags are set on metrics in this section:

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

The following tag is also set:

* :ref:`quantile <metrics_tag_quantile>`

The metric names are:

* user.<site_id>.task.ready.duration

  **type**: gauge

  Histogram summary for task ready duration, corresponding to the
  :ref:`ready_time <log_task_ready_time>` field in the logs.

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

* user.<site_id>.upstream.traffic.in.bytes

  **type**: count

  Total bytes received from the upstream side.

* user.<site_id>.upstream.traffic.in.packets

  **type**: count

  Total datagram packets received from the upstream side.
  This metric is not available for stream-oriented transports.

* user.<site_id>.upstream.traffic.out.bytes

  **type**: count

  Total bytes sent to the upstream side.

* user.<site_id>.upstream.traffic.out.packets

  **type**: count

  Total datagram packets sent to the upstream side.
  This metric is not available for stream-oriented transports.
