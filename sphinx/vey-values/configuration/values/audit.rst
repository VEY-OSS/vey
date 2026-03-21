.. _configure_audit_value_types:

*****
Audit
*****

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: not currently used
   - ``vey-keyless``: not currently used
   - ``vey-statsd``: not currently used

This page documents value types related to auditing, ICAP integration, and
stream detouring.

.. _conf_value_audit_icap_service_config:

icap service config
===================

**type**: map | str

Configuration for an ICAP service.

If the value is a string, it is interpreted as the ``url`` field described
below.

If the value is a map, the following keys are supported:

* url

  **required**, **type**: :ref:`url str <conf_value_url_str>`

  ICAP service URL. The scheme must be either ``icap`` or ``icaps``.
  When the scheme is ``icaps``, a default TLS client configuration is used.

* use_unix_socket

  **optional**, **type**: :ref:`absolute path <conf_value_absolute_path>`

  UNIX domain socket path to try before falling back to TCP.

  If the path cannot be connected, the TCP address from the URL is used as a
  fallback.

  **default**: not set

  .. availability::


     - ``vey-proxy``: available since ``1.12.0``

* tls_client

  **optional**, **type**: :ref:`rustls client config <conf_value_rustls_client_config>`

  Enables TLS and configures it. TLS is enabled even when the URL scheme is
  ``icap``.

  **default**: not set for 'icap://' url, default one for 'icaps://' url

  .. availability::


     - ``vey-proxy``: available since ``1.9.9``

* tls_name

  **optional**, **type**: :ref:`tls name <conf_value_tls_name>`

  TLS server name used to verify the peer certificate.

  **default**: same as the host port in url

  .. availability::


     - ``vey-proxy``: available since ``1.9.9``

* tcp_keepalive

  **optional**, **type**: :ref:`tcp keepalive <conf_value_tcp_keepalive>`

  TCP keepalive configuration for the connection to the ICAP server.

  **default**: enabled with default value

* icap_connection_pool

  **optional**, **type**: :ref:`connection pool <conf_value_connection_pool_config>`

  Connection-pool configuration.

  **default**: set with default value

* icap_max_header_size

  **optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Maximum header size accepted when parsing responses from the ICAP server.

  **default**: 8KiB

* no_preview

  **optional**, **type**: bool

  Set to ``true`` to disable ICAP preview.

  **default**: false

  .. availability::


     - ``vey-proxy``: available since ``1.11.6``

* preview_data_read_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Timeout used when reading preview data.
  If the read times out, preview is not used for the request sent to the ICAP
  server.

  **default**: 4s

* respond_shared_names

  **optional**, **type**: :ref:`http header name <conf_value_http_header_name>` or seq of this

  Headers returned by the ICAP server in a ``REQMOD`` response that should be
  forwarded into the following ``RESPMOD`` request.

  This option currently applies only to the ``REQMOD`` service.

  **default**: not set

* bypass

  **optional**, **type**: bool

  Controls whether processing should fall back to bypass mode if the ICAP
  server cannot be reached.

  **default**: false

.. _conf_value_audit_stream_detour_service_config:

stream detour service config
============================

**type**: map | str | int

Configuration for the stream detour helper service.

If the value is a string, it is interpreted as the ``peer`` field described
below.

If the value is a map, the following keys are supported:

* peer

  **optional**, **type**: :ref:`upstream str <conf_value_upstream_str>`

  Set the peer address.

  **default**: 127.0.0.1:2888

* tls_client

  **optional**, **type**: :ref:`rustls client config <conf_value_rustls_client_config>`

  Enable tls and set the config.

  **default**: not set

* tls_name

  **optional**, **type**: :ref:`tls name <conf_value_tls_name>`

  Set the tls server name to verify peer certificate.

  **default**: not set

* connection_pool

  **optional**, **type**: :ref:`connection pool <conf_value_connection_pool_config>`

  Set the connection pool config.

  **default**: set with default value

* connection_reuse_limit

  **optional**, **type**: :ref:`nonzero usize <conf_value_nonzero_usize>`

  Set how many times a single QUIC connection will be reused.
  The max allowed streams on this QUIC connection should be double of this value.

  **default**: 16

* quic_transport

  **optional**, **type**: :ref:`quinn transport <conf_value_quinn_transport>`

  Set the transport config for quinn.

  **default**: set with default value

  .. availability::


     - ``vey-proxy``: available since ``1.9.9``

* stream_open_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Set the timeout to open QUIC streams to the detour server.

  **default**: 30s

* request_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Set the timeout to get detour action response from the detour server after open the streams.

  **default**: 60s

* socket_buffer

  **optional**, **type**: :ref:`socket buffer config <conf_value_socket_buffer_config>`

  Set the socket buffer config for the socket to peer.

  **default**: not set

.. availability::


   - ``vey-proxy``: available since ``1.9.8``
