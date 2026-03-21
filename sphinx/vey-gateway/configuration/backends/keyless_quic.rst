.. _configuration_backend_keyless_quic:

************
keyless_quic
************

A keyless backend that connects to upstream peers over QUIC.

This backend type is valid only for keyless tasks.

Config Keys
===========

The following common keys are supported:

* :ref:`discover <conf_backend_common_discover>`
* :ref:`discover_data <conf_backend_common_discover_data>`
* :ref:`extra_metrics_tags <conf_backend_common_extra_metrics_tags>`

tls_client
----------

**required**, **type**: :external+values:ref:`rustls client config <conf_value_rustls_client_config>`

Set the TLS configuration for the local QUIC client.

**default**: not set

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

Set the TLS server name used to verify upstream certificates.

If not set, the peer IP will be used.

**default**: not set

duration_stats
--------------

**optional**, **type**: :external+values:ref:`histogram metrics <conf_value_histogram_metrics>`

Configure histogram metrics for connection duration.

**default**: set with default value

request_buffer_size
-------------------

**optional**, **type**: usize

Set the size of the local request queue. New connections are opened when the
queue is full.

**default**: 128

response_recv_timeout
---------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the timeout for waiting for an upstream response.

On timeout, the request is dropped from the local buffer and an internal error
response is returned to the client.

**default**: 4s

connection_max_request_count
----------------------------

**optional**, **type**: usize

Set the maximum number of requests handled by a single upstream stream.

**default**: 4000

.. versionadded:: 0.3.4

connection_alive_time
---------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the maximum lifetime of a single upstream stream.

**default**: 1h

.. versionadded:: 0.3.4

graceful_close_wait
-------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the graceful wait time before closing a live connection.

**default**: 10s

connection_pool
---------------

**optional**, **type**: :external+values:ref:`connection pool <conf_value_connection_pool_config>`

Set the connection-pool configuration.

**default**: enabled with ``max_idle = 2048`` and ``min_idle = 128``

.. versionadded:: 0.3.5

quic_transport
--------------

**optional**, **type**: :external+values:ref:`quinn transport <conf_value_quinn_transport>`

Set the Quinn transport configuration.

**default**: set with default value

.. versionadded:: 0.3.5

concurrent_streams
------------------

**optional**, **type**: usize

Set how many bidirectional streams are used on a single QUIC connection.

**default**: 4

wait_new_channel
----------------

**optional**, **type**: bool

Set whether requests should wait for a new usable stream when no live stream is
available.

**default**: false

.. versionadded:: 0.3.5

socket_buffer
-------------

**optional**, **type**: :external+values:ref:`socket buffer config <conf_value_socket_buffer_config>`

Set the UDP socket buffer configuration.

**default**: not set
