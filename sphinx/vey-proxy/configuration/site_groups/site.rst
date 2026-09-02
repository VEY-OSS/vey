.. _configuration_site:

****
Site
****

A site is one origin: one upstream and optional ingress / egress TLS.

Sites live in a :ref:`site group <configuration_site_group>` under
:ref:`static_sites <conf_site_group_static_sites>`. Match keys
(``exact_match``, ``suffix_match`` / ``child_match``, ``set_default``) belong
to the host-match wrapper; the keys below belong to the site itself.

This is not a :ref:`user site <configuration_auth_user_site>`. User sites are
per-user destination overrides on a forward-proxy user. A site here is an
origin selected by Host / SNI on a reverse-proxy server such as
``http_expose``.

.. versionadded:: 1.15.0

.. _conf_site_id:

id
--

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Site ID. It must be unique inside the site group. Reloading the site group
reuses this site's runtime (stats, limiter) when the ID is unchanged.

**alias**: ``name``

.. _conf_site_upstream:

upstream
--------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>`

Target upstream address. The default port is ``80`` and may be omitted.

.. _conf_site_tls_server:

tls_server
----------

**optional**, **type**: :external+values:ref:`openssl server config <conf_value_openssl_server_config>`

TLS server configuration for this site.

If unset, :ref:`global_tls_server <configuration_server_http_rproxy_global_tls_server>`
on the ``http_expose`` server is used when TLS is enabled.

**default**: not set

.. _conf_site_tls_client:

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

TLS parameters used when connecting to the upstream over HTTPS.
An empty map enables the default client configuration.

**default**: not set, which means plaintext HTTP to the upstream

.. _conf_site_tls_name:

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

SNI and certificate name used to verify the upstream.

If unset, the host part of ``upstream`` is used.

**default**: not set

.. _conf_site_tcp_sock_speed_limit:

tcp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Per-connection speed limit for this site. The effective limit is the smaller of
this value, the ``http_expose`` server limit, and the visitor user limit.

**default**: no extra limit

.. _conf_site_request_rate_limit:

request_rate_limit
------------------

**optional**, **type**: :external+values:ref:`rate limit quota <conf_value_rate_limit_quota>`

Rate limit for requests to this site.

**default**: no limit, **alias**: request_limit_quota

.. _conf_site_request_max_alive:

request_max_alive
-----------------

**optional**, **type**: usize, **alias**: request_alive_max

Maximum number of concurrent requests for this site.

**default**: no limit
