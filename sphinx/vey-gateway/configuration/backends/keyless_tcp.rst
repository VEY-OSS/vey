.. _configuration_backend_keyless_tcp:

***********
keyless_tcp
***********

A keyless backend that connects to upstream peers over TCP or TLS.

This backend type is valid only for keyless tasks.

Config Keys
===========

The following common keys are supported:

* :ref:`discover <conf_backend_common_discover>`
* :ref:`discover_data <conf_backend_common_discover_data>`
* :ref:`extra_metrics_tags <conf_backend_common_extra_metrics_tags>`

tls_client
----------

**optional**, **type**: :external+values:ref:`rustls client config <conf_value_rustls_client_config>`

Enable TLS and configure the local TLS client.

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

Configure histogram metrics for TCP connect duration.

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

Set the maximum number of requests handled by a single upstream connection.

**default**: 4000

.. versionadded:: 0.3.4

connection_alive_time
---------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the maximum lifetime of a single upstream connection.

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

**default**: enabled with ``max_idle = 8192`` and ``min_idle = 256``

.. versionadded:: 0.3.5

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Set TCP keepalive.

**default**: no keepalive set

wait_new_channel
----------------

**optional**, **type**: bool

Set whether requests should wait for a new connection when no live connection
is available.

**default**: false

.. versionadded:: 0.3.5
