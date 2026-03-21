.. _configuration_log_driver_fluentd:

fluentd
=======

The ``fluentd`` driver configuration is a map.

Use this driver to send logs to Fluentd or Fluent Bit over the
`Forward Protocol`_.

.. _Forward Protocol: https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1

Task logs are emitted with the Fluentd tag ``vey-gateway.Task``.

The keys are described below.

address
-------

**optional**, **type**: :external+values:ref:`env sockaddr str <conf_value_env_sockaddr_str>`

Set the TCP address of the Fluentd server.

**default**: 127.0.0.1:24224

bind_ip
-------

**optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

Set the local IP address used for the outbound socket.

**default**: not set

shared_key
----------

**optional**, **type**: str

Set the shared key used during Fluentd authentication.

If this key is not set, the shared-key handshake is skipped.

**default**: not set

username
--------

**optional**, **type**: str

Set the username for Fluentd authentication.

This is used only when the server requires user authentication.

**default**: not set

password
--------

**optional**, **type**: str

Set the password for Fluentd authentication.

This is used only when the server requires user authentication.

**default**: not set

hostname
--------

**optional**, **type**: str

Set the hostname sent to the Fluentd peer.

**default**: local hostname

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Set TCP keepalive for the connection to the Fluentd server.

**default**: enabled with system default values

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Enable TLS and configure the client parameters.

**default**: not set

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

Set the TLS server name used to verify the peer certificate.

**default**: not set

connect_timeout
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the timeout for connecting to the Fluentd server, including TCP connect,
TLS handshake, and Fluentd handshake.

**default**: 10s

connect_delay
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the retry delay after a connection attempt fails. Messages received during
this delay are dropped.

**default**: 10s

write_timeout
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the write timeout for each message. Timed-out messages are dropped.

**default**: 1s

flush_interval
--------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the flush interval for the connection to the Fluentd server.

**default**: 100ms

retry_queue_len
---------------

**optional**, **type**: usize

Set how many events are retained for retry after connect or write failures.
Events that hit the write timeout are dropped immediately.

**default**: 10
