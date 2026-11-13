.. _metrics_server:

##############
Server Metrics
##############

Server metrics describe lookup activity and listen sockets for each configured
server.

Request
=======

The following tags are present:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`
* :ref:`server <metrics_tag_server>`
* :ref:`online <metrics_tag_online>`

The metric names are:

* server.request.total

  **type**: count

  Number of datagrams delivered to this server. This includes invalid requests
  and lookups that match no database record.

* server.request.invalid

  **type**: count

  Number of datagrams that were not a valid locate request, or that did not
  contain an IP address.

* server.lookup.not_found

  **type**: count

  Number of valid lookups that matched neither the country table nor the ASN
  table. No response is sent. A match in either table is not counted here.

* server.request.alive

  **type**: gauge

  Number of lookups currently being handled by this server.

* server.response.total

  **type**: count

  Number of response datagrams encoded for a client. A lookup counted by
  ``server.lookup.not_found`` produces no response and is not counted here.
  An encoding error is logged and is not counted here.

* server.response.fail

  **type**: count

  Number of encoded responses that could not be written back to the client.

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

Connection counters such as ``listen.accepted``, ``listen.dropped``,
``listen.timeout``, and ``listen.failed`` stay at zero for these datagram
listeners, so they are not emitted.
