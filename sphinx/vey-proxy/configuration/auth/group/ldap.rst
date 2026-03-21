.. _configuration_auth_user_group_ldap:

LDAP
====

User-group type that authenticates users against a remote LDAP server through
simple bind.

The following common keys are supported:

* :ref:`name <conf_auth_user_group_name>`
* :ref:`type <conf_auth_user_group_type>`
* :ref:`static users <conf_auth_user_group_static_users>`
* :ref:`source <conf_auth_user_group_source>`
* :ref:`cache <conf_auth_user_group_cache>`
* :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
* :ref:`anonymous_user <conf_auth_user_group_anonymous_user>`

ldap_url
--------

**required**, **type**: LDAP URL

LDAP URL in the form ``<schema>://<server_name>:[<port>]/<base_dn>``.
The schema must be either ``ldap`` or ``ldaps``. The default port is ``389``
for ``ldap`` and ``636`` for ``ldaps``.

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

TLS parameters for the local TLS client.
If set to an empty map, the default configuration is used.

If the LDAP URL uses the ``ldap`` scheme and this field is set, ``STARTTLS`` is
used.

If the schema is "ldaps", a default value will be used if not set.

**default**: not set

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

TLS server name used to verify peer certificates.

If not set, the host part of each peer will be used.

**default**: not set

username_attribute
------------------

**optional**, **type**: string

LDAP attribute name used for usernames.

The most common value is `uid` while some LDAP servers may use `cn`.

**default**: uid

unmanaged_user
--------------

**optional**, **type**: :ref:`user <configuration_auth_user>`

Configures and enables unmanaged users.

This is a template user configuration for users who authenticate successfully
with LDAP but are not defined in either the static or dynamic user lists.

If not set, only static or dynamic users will be allowed.

**default**: not set

max_message_size
----------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Maximum message size accepted when parsing responses from the LDAP server.

**default**: 256

connect_timeout
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for establishing the TCP connection to the LDAP server.

**default**: 4s

response_timeout
----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout while reading responses from the LDAP server.

**default**: 2s

connection_pool
---------------

**optional**, **type**: :external+values:ref:`connection pool <conf_value_connection_pool_config>`

Connection-pool configuration.

**default**: set with default value

queue_channel_size
------------------

**optional**, **type**: usize

Queue channel size used when authenticating a client request against the LDAP
server.

**default**: 64

queue_wait_timeout
------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout while authenticating a client request against the LDAP server.

**default**: 4s

cache_user_count
----------------

**optional**, **type**: usize

Maximum number of users stored in the thread-local LRU cache.

**default**: 128

cache_expire_time
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Expiration time for valid passwords in the thread-local LRU cache.

**default**: 5min
