.. _configuration_server:

******
server
******

The following keys are supported in a single keyless server:
The following keys are supported in a single keyless server definition:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Server name.

shared_logger
-------------

**optional**, **type**: ascii

Makes this server use a logger running on a shared thread.

**default**: not set

extra_metrics_tags
------------------

**optional**, **type**: :external+values:ref:`static metrics tags <conf_value_static_metrics_tags>`

Extra metric tags added to server statistics.

**default**: not set

listen
------

**required**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listen configuration for this server.

tls_server
----------

**optional**, **type**: :external+values:ref:`openssl server config <conf_value_openssl_server_config>`

Enable TLS on the listening socket and configure TLS parameters.

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
