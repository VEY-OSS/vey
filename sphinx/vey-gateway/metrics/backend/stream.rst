.. _metrics_backend_stream:

######################
Stream Backend Metrics
######################

Connection Metrics
==================

No additional tags.

The metric names are:

* backend.stream.connection.attempt

  **type**: count

  The number of connection attempts.

* backend.stream.connection.established

  **type**: count

  The number of successfully established connections.

Duration Metrics
================

The following additional tag is present:

* :ref:`quantile <metrics_tag_quantile>`

The metric names are:

* backend.stream.connect.duration

  **type**: gauge

  TCP connect duration statistics.
