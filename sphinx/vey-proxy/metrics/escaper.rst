.. _metrics_escaper:

###############
Escaper Metrics
###############

Escaper-side metrics describe activity on the remote or upstream side of a task.

For **non-route escapers**, *request* and *traffic* metrics are available.
For **route escapers**, *route* metrics are available.

The following tags are present on all escaper metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* escaper

  The escaper name.

Request
=======

No additional fixed tags. Any extra tags configured on the escaper are also
included.

The metric names are:

* escaper.task.total

  **type**: count

  Total number of tasks that use this escaper.

* escaper.connection.attempt

  **type**: count

  Number of connection attempts made to the remote side.

* escaper.connection.establish

  **type**: count

  Number of connections established to the remote side.

* escaper.tcp.connect.attempt

  **type**: count

  Number of TCP connection attempts to the next peer.

  .. versionadded:: 1.11.1

* escaper.tcp.connect.establish

  **type**: count

  Number of TCP connections successfully established to the next peer and used
  by tasks.

  .. versionadded:: 1.11.1

* escaper.tcp.connect.success

  **type**: count

  Number of successful TCP connection attempts to the next peer.

  .. note::

    This differs from *escaper.tcp.connect.establish*. During Happy Eyeballs,
    multiple connection attempts may succeed, but only one is ultimately used
    by the task.

  .. versionadded:: 1.11.1

* escaper.tcp.connect.error

  **type**: count

  Number of failed TCP connection attempts to the next peer due to an error.

  .. versionadded:: 1.11.1

* escaper.tcp.connect.timeout

  **type**: count

  Number of TCP connection attempts to the next peer that failed due to
  timeout.

  .. versionadded:: 1.11.1

* escaper.tls.handshake.success

  **type**: count

  Number of successful TLS handshakes with the next proxy peer.

  .. versionadded:: 1.11.1

* escaper.tls.handshake.error

  **type**: count

  Number of TLS handshakes with the next proxy peer that failed due to an
  error.

  .. versionadded:: 1.11.1

* escaper.tls.handshake.timeout

  **type**: count

  Number of TLS handshakes with the next proxy peer that failed due to timeout.

  .. versionadded:: 1.11.1

* escaper.tls.peer.closure.orderly

  **type**: count

  Number of TLS warning alerts received from the peer, including
  ``close_notify`` and ``user_canceled``.

  .. note:: You may see user_canceled followed by a close_notify on one connection.

  .. versionadded:: 1.11.4

* escaper.tls.peer.closure.abortive

  **type**: count

  Number of TLS error alerts received from the peer, indicating an abortive
  connection closure.

  .. versionadded:: 1.11.4

* escaper.forbidden.ip_blocked

  **type**: count

  Number of connection attempts blocked because the resolved IP was forbidden.

  This metric is also added to user forbidden metrics when possible.

Traffic
=======

The following tag is also set:

* :ref:`transport <metrics_tag_transport>`

Any extra tags configured on the escaper are also included.

These I/O metrics include traffic above the transport layer. TLS payload and
TLS framing are therefore counted together.

The metric names are:

* escaper.traffic.in.bytes

  **type**: count

  Total bytes received from the remote side through this escaper.

* escaper.traffic.in.packets

  **type**: count

  Total datagram packets received from the remote side through this escaper.
  This metric is not available for stream-oriented transports.

* escaper.traffic.out.bytes

  **type**: count

  Total bytes sent to the remote side through this escaper.

* escaper.traffic.out.packets

  **type**: count

  Total datagram packets sent to the remote side through this escaper.
  This metric is not available for stream-oriented transports.

Route
=====

No additional tags.

The metric names are:

* route.request.passed

  **type**: count

  Number of requests successfully routed.

* route.request.failed

  **type**: count

  Number of requests that failed during route selection.
