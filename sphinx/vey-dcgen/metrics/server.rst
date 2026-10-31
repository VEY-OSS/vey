.. _metrics_server:

##############
Server Metrics
##############

Server metrics describe certificate requests and listen sockets for each
configured server. Backend metrics describe the certificate generator itself
and are not tagged with a server name.

Request
=======

The following tags are present:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`server <metrics_tag_server>`

The metric names are:

* server.request_total

  **type**: count

  Number of datagrams delivered to this server. This includes invalid requests.

* server.request_invalid

  **type**: count

  Number of datagrams that were not a valid certificate request. No response
  is sent.

* server.request_alive

  **type**: gauge

  Number of requests currently being handled by this server, including time
  spent waiting for certificate generation.

* server.response_total

  **type**: count

  Number of response datagrams encoded for a client. An encoding error is
  logged and is not counted here. A failed certificate generation is logged
  and is not counted here.

* server.response_fail

  **type**: count

  Number of encoded responses that could not be written back to the client.

Backend
=======

The following tag is present:

* :ref:`daemon_group <metrics_tag_daemon_group>`

The metric names are:

* backend.refresh_total

  **type**: count

  Number of attempts to refresh the certificate builder clock and key.

* backend.refresh_ok

  **type**: count

  Number of those refresh attempts that succeeded.

* backend.request_total

  **type**: count

  Number of certificate generations started.

* backend.request_ok

  **type**: count

  Number of certificate generations that produced a certificate and key.

* backend.request_duration

  **type**: gauge

  Time from receiving a request datagram to writing the response, in
  nanoseconds. The :ref:`quantile <metrics_tag_quantile>` tag selects the
  histogram bucket.

Listen
======

The following tags are present:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`
* :ref:`server <metrics_tag_server>`
* :ref:`online <metrics_tag_online>`

The metric names are:

* listen.instance.count

  **type**: gauge

  Number of listening sockets currently running for this server. A dummy server
  stays at zero.

Datagram listeners also report the counters below. A counter that has stayed at
zero is not emitted.

* listen.datagram.recv.total

  **type**: count

  Number of datagrams read from the listen socket.

* listen.datagram.recv.dropped

  **type**: count

  Number of received datagrams discarded before handling. A datagram is dropped
  when it has no source address.

* listen.datagram.recv.ok

  **type**: count

  Number of received datagrams delivered to the server.

* listen.datagram.send.queued

  **type**: count

  Number of datagrams queued for sending on the listen socket.

* listen.datagram.send.failed

  **type**: count

  Number of queued datagrams the listen socket failed to send.

* listen.datagram.send.ok

  **type**: count

  Number of datagrams written to the listen socket.
