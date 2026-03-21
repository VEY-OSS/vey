.. _metrics_backend_keyless:

#######################
Keyless Backend Metrics
#######################

Connection Metrics
==================

No additional tags.

The metric names are:

* backend.keyless.connection.attempt

  **type**: count

  The number of connection attempts.

* backend.keyless.connection.established

  **type**: count

  The number of successfully established connections.

* backend.keyless.channel.alive

  **type**: gauge

  The number of live channels. A channel may be a TCP connection or a QUIC
  stream.

Request Metrics
===============

* backend.keyless.request.recv

  **type**: count

  The number of requests received.

* backend.keyless.request.send

  **type**: count

  The number of requests sent to the upstream peer.

* backend.keyless.request.drop

  **type**: count

  The number of requests dropped internally.

* backend.keyless.request.timeout

  **type**: count

  The number of requests that timed out while waiting for a response.

* backend.keyless.response.recv

  **type**: count

  The number of responses received from the upstream peer.

* backend.keyless.response.send

  **type**: count

  The number of responses sent to the client.

* backend.keyless.response.drop

  **type**: count

  The number of responses dropped internally.

Duration Metrics
================

The following additional tag is present:

* :ref:`quantile <metrics_tag_quantile>`

The metric names are:

* backend.keyless.connect.duration

  **type**: gauge

  Connection duration statistics.

* backend.keyless.wait.duration

  **type**: gauge

  Internal queue wait-duration statistics.

* backend.keyless.response.duration

  **type**: gauge

  Upstream response-duration statistics.
