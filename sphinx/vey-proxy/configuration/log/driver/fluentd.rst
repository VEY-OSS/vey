.. _configuration_log_driver_fluentd:

fluentd
=======

The ``fluentd`` driver configuration is a map.

It sends logs to Fluentd or Fluent Bit by using the `Forward Protocol`_.

.. _Forward Protocol: https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1

The Fluentd event tag is ``vey-proxy.Task``, ``vey-proxy.Escape``, or
``vey-proxy.Resolve`` depending on the log type.

The keys are described below.

address
-------

**optional**, **type**: :external+values:ref:`env sockaddr str <conf_value_env_sockaddr_str>`

TCP address of the Fluentd server.

**default**: 127.0.0.1:24224

bind_ip
-------

**optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

Local IP address to bind before connecting.

**default**: not set

shared_key
----------

**optional**, **type**: str

Shared key used when authentication is required.

If this is not set, the shared-key handshake stage is skipped.

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

Custom hostname to report to the Fluentd server.

**default**: local hostname

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

TCP keepalive configuration for the connection to the Fluentd server.

**default**: enabled with system default values

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Enables TLS and configures it.

**default**: not set

.. versionchanged:: 1.7.35 switch to use rustls
.. versionchanged:: 1.11.1 switch to use OpenSSL

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

TLS server name used to verify the peer certificate.

**default**: not set

.. versionadded:: 1.7.35

connect_timeout
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for establishing the Fluentd connection, including TCP connect, TLS
handshake, and Fluentd handshake.

**default**: 10s

connect_delay
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Delay before retrying after a connection failure. Messages received during this
delay are dropped.

**default**: 10s

write_timeout
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Write timeout for each message. Timed-out messages are dropped.

default: 1s

flush_interval
--------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Flush interval for the Fluentd connection.

**default**: 100ms

retry_queue_len
---------------

**optional**, **type**: usize

Maximum number of events queued for retry after connection or write failures.
Messages that fail due to write timeout are dropped immediately.

**default**: 10
