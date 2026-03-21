.. _configuration_log_driver_fluentd:

fluentd
=======

The fluentd driver configuration is a map.

It sends logs to Fluentd or Fluent Bit using the `Forward Protocol`_.

.. _Forward Protocol: https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1

The Fluentd event tag for these logs is ``vey-keyless.Task``.

The keys are described below.

address
-------

**optional**, **type**: :external+values:ref:`env sockaddr str <conf_value_env_sockaddr_str>`

TCP address of the Fluentd server.

**default**: 127.0.0.1:24224

bind_ip
-------

**optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

Local IP address to bind for the client socket.

**default**: not set

shared_key
----------

**optional**, **type**: str

Shared key used when authentication is required.

The handshake stage will be skipped if shared key is not set.

**default**: not set

username
--------

**optional**, **type**: str

Username used when authorization is required.

This will only be used if authorization is required by the server.

**default**: not set

password
--------

**optional**, **type**: str

Password used when authorization is required.

This will only be used if authorization is required by the server.

**default**: not set

hostname
--------

**optional**, **type**: str

Custom hostname.

**default**: local hostname

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

TCP keepalive configuration for the connection to the Fluentd server.

**default**: enabled with system default values

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Enable TLS and set the client configuration.

**default**: not set

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

TLS server name used to verify the peer certificate.

**default**: not set

connect_timeout
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for establishing the Fluentd connection, including TCP connect, TLS
handshake, and Fluentd handshake.

**default**: 10s

connect_delay
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Delay before retrying after a failed connection attempt. Incoming messages are
dropped during that interval.

**default**: 10s

write_timeout
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Write timeout for each message. Timed-out messages are dropped.

**default**: 1s

flush_interval
--------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Flush interval for the connection to the Fluentd server.

**default**: 100ms

retry_queue_len
---------------

**optional**, **type**: usize

Maximum number of events queued for retry after connection or write failures.
Events that hit the write timeout are dropped immediately.

**default**: 10
