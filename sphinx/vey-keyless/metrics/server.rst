.. _metrics_server:

##############
Server Metrics
##############

Server metrics describe listener, task, and request activity on the keyless server.

The following tags are present on all server metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* server

  Server name.

* online

  Whether the server is online. The value is either ``y`` or ``n``.

Listen
======

No extra tags.

The metric names are:

* listen.instance.count

  **type**: gauge

  Number of listening sockets.

* listen.accepted

  **type**: count

  Number of accepted client connections.

* listen.dropped

  **type**: count

  Number of client connections dropped early by ACL rules.

* listen.timeout

  **type**: count

  Number of client connections that timed out during early protocol negotiation, such as TLS setup.

* listen.failed

  **type**: count

  Number of accept errors.

Task
====

A task is a keyless client connection.

No additional fixed tags are present. Extra tags configured on the server are added.

The metrics names are:

* server.task.total

  **type**: count

  Number of valid tasks spawned. Each accepted client connection is promoted to a task.

* server.task.alive

  **type**: gauge

  Number of live tasks currently running for this server. During normal systemd
  shutdown, servers with running tasks enter offline mode and wait for those
  tasks to finish.

Request
=======

Extra tags configured on the server are added.

The following extra tags are used by request metrics:

* request

  Keyless request type. Present on all request metrics.

  The values are:

    - no_op
    - ping_pong
    - rsa_decrypt
    - rsa_sign
    - rsa_pss_sign
    - ecdsa_sign
    - ed25519_sign

* reason

  Failure reason for unsuccessful keyless requests.

  The values are:

    - key_not_found
    - crypto_fail
    - bad_op_code
    - format_error
    - other_fail

* :ref:`quantile <metrics_tag_quantile>`

The metric names are:

* server.request.total

  **type**: count

  Total number of new requests.

* server.request.alive

  **type**: gauge

  Number of keyless requests currently being processed.

* server.request.passed

  **type**: count

  Number of successful keyless requests.

* server.request.failed

  **type**: count

  Number of failed keyless requests. The ``reason`` tag is added.

* server.request.duration

  **type**: gauge

  Histogram statistics for keyless request processing duration, corresponding
  to :ref:`process_time <log_request_process_time>` in request logs.
