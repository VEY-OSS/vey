.. _configuration_server_cloudflare:

cloudflare
==========

This server speaks the Cloudflare Keyless protocol and runs the private key
operations. It is the only key-operation server type currently supported.

It is the default server type, so ``type`` may be omitted.

**alias**: cloudflare_keyless

The following common keys are supported:

* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listen configuration for this server.

If not set, this server has no listening socket of its own and only serves the
connections sent by a port server, such as
:ref:`plain_tls_port <configuration_server_plain_tls_port>`.

**default**: not set

.. versionchanged:: 0.6.0 became optional

tls_server
----------

**optional**, **type**: :external+values:ref:`openssl server config <conf_value_openssl_server_config>`

Enable TLS on the listening socket and configure TLS parameters.

This uses OpenSSL. To terminate TLS with rustls instead, put a
:ref:`plain_tls_port <configuration_server_plain_tls_port>` server in front of
this one.

This requires ``listen`` to be set.

**default**: disabled

multiplex_queue_depth
---------------------

**optional**, **type**: usize

Enable request multiplexing and set the queue depth.

This is required when you want to use multiple worker backends.

**default**: not set

request_read_timeout
--------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

**default**: 100ms

Timeout for reading a single request from the client connection.

duration_stats
--------------

**optional**, **type**: :external+values:ref:`histogram metrics <conf_value_histogram_metrics>`

Histogram-metric configuration for request-duration statistics.

**default**: set with default value

async_op_timeout
----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for the async operation of a single request.

**default**: 1s

concurrency_limit
-----------------

**optional**, **type**: usize

Request concurrency limit. Extra requests wait in the queue.

**default**: not limited
