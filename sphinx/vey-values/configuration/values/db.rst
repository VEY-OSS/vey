.. _configure_db_value_types:

**
DB
**

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: not currently used
   - ``vey-statsd``: not currently used

.. _conf_value_db_redis:

redis
=====

**yaml type**: map

Redis server address and connection parameters.

The same map is also accepted by loaders that use a nested Redis config value,
such as dynamic peer sources and TLS-ticket key sources.

The following fields are supported:

* addr

  **required**, **type**: :ref:`upstream str <conf_value_upstream_str>`

  Address of the Redis instance. The default port is ``6379`` and may be
  omitted.

* tls_client

  **optional**, **type**: :ref:`rustls client config <conf_value_rustls_client_config>`, **alias**: tls

  Enables TLS and configures it.

  **default**: not set

  .. availability::


     - ``vey-proxy``: available since ``1.9.7``

* tls_name

  **optional**, **type**: :ref:`tls name <conf_value_tls_name>`

  TLS server name used to verify the peer certificate.

  **default**: not set

  .. availability::


     - ``vey-proxy``: available since ``1.9.7``

* db

  **optional**, **type**: int

  Database index.

  **default**: 0

* username

  **optional**, **type**: str

  Username for Redis 6 or later when ACLs are enabled.

  **default**: not set

* password

  **optional**, **type**: str

  Password.

  **default**: not set

* connect_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Connect timeout.

  **default**: 5s

* response_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Read timeout for Redis command responses.

  **default**: 2s, **alias**: read_timeout

Example:

.. code-block:: yaml

   redis:
     addr: redis.example.net:6379
     tls: {}
     tls_name: redis.example.net
     db: 3
     username: app
     password: secret
     connect_timeout: 3s
     read_timeout: 1s
