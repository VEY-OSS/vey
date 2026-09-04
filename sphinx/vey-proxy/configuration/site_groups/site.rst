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
``http_expose`` or ``tls_proxy``.

.. versionadded:: 1.15.0

.. _conf_site_id:

id
--

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Site ID. It must be unique inside the site group. Reloading the site group
reuses this site's runtime (stats, limiter) when the ID is unchanged. The ID
is exported as the ``site`` tag on :ref:`site metrics <metrics_site>`.

**alias**: ``name``

.. _conf_site_owner:

owner
-----

**optional**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

The tenant this site belongs to. Reverse-proxy servers look this name up in
the site group's :ref:`tenant_user_group <conf_site_group_tenant_user_group>`
and attach the matching user as the site tenant for the request. That tenant
supplies rate limits, idle limits, and egress overrides (connect, keepalive,
path selection, resolve strategy) that are then constrained by this site.

It is also exported as the ``user`` tag on :ref:`site metrics
<metrics_site>`. When unset, that tag is ``-`` and the request has no tenant.

If the name is set but ``tenant_user_group`` is unset, or the user is not
found in that group, the site is still served without a tenant.

**default**: not set, **alias**: ``tenant``

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

``http_expose`` uses this when TLS is enabled; if unset,
:ref:`global_tls_server <configuration_server_http_rproxy_global_tls_server>`
on that server is used.

:ref:`tls_proxy <configuration_server_tls_proxy>` requires this key. Sites
without it are skipped by ``tls_proxy``; there is no server-level fallback
certificate.

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

.. _conf_site_dpi_protocol:

dpi_protocol
------------

**optional**, **type**: str

Inner protocol after TLS termination, used as a DPI hint by
:ref:`tls_proxy <configuration_server_tls_proxy>` when that server has an
auditor. Recognised values are the same protocol names as protocol
inspection (``http``, ``smtp``, ``imap``, …). This is the protocol
**inside** TLS; do not set ``https``.

``http_expose`` ignores this key. ``tls_proxy`` ignores it when no
auditor is configured.

**default**: not set, inspect from the port map and the first bytes

.. _conf_site_tcp_sock_speed_limit:

tcp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Per-connection speed limit for this site. The effective limit is the smaller of
this value, the reverse-proxy server limit, and any attached tenant or visitor
user limit.

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

.. _conf_site_task_idle_max_count:

task_idle_max_count
-------------------

**optional**, **type**: usize

The task is closed once the idle check reports ``IDLE`` this many times.

The effective count is the **minimum** of the values set on:

* tenant user
* this origin site
* visitor user

Layers that omit the key are skipped. The result overwrites the server
:ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`.
If none of them set it, the server value is used.

The idle-check interval can only be configured at the server level,
see :ref:`server task_idle_check_interval <conf_server_common_task_idle_check_interval>`.

**default**: not set

.. _conf_site_tcp_connect:

tcp_connect
-----------

**optional**, **type**: :external+values:ref:`tcp connect <conf_value_tcp_connect>`

Origin-site TCP connect parameters. These apply to *direct* escapers and are
further constrained by escaper-level settings.

When a tenant user is present, the tenant value is limited to this site
value. A visitor user does not change origin connect
parameters.

**default**: not set

.. _conf_site_tcp_remote_keepalive:

tcp_remote_keepalive
--------------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

TCP keepalive for the remote TCP socket to this origin.

When a tenant user is present, the tenant keepalive is adjusted to this
site value. A visitor user does not change origin keepalive.

**default**: no keepalive set

.. _conf_site_tcp_remote_misc_opts:

tcp_remote_misc_opts
--------------------

**optional**, **type**: :external+values:ref:`tcp misc sock opts <conf_value_tcp_misc_sock_opts>`

Miscellaneous TCP socket options for the remote TCP socket to this origin.

When a tenant user is present, the tenant options are adjusted to this
site value. A visitor user does not change origin socket options.

**default**: not set

.. _conf_site_udp_remote_misc_opts:

udp_remote_misc_opts
--------------------

**optional**, **type**: :external+values:ref:`udp misc sock opts <conf_value_udp_misc_sock_opts>`

Miscellaneous UDP socket options for the remote UDP socket to this origin.

When a tenant user is present, the tenant options are adjusted to this
site value. A visitor user does not change origin socket options.

**default**: not set

.. _conf_site_resolve_strategy:

resolve_strategy
----------------

**optional**, **type**: :external+values:ref:`resolve strategy <conf_value_resolve_strategy>`

Custom resolve strategy for this origin, constrained by the strategy allowed
by the escaper.

If this site does not set it, the tenant user's strategy is used when
present. Visitor users are not consulted for origin resolve.

This site does not accept ``resolve_redirection``.

**default**: no custom resolve strategy is set

.. _conf_site_egress_path_id_map:

egress_path_id_map
------------------

**optional**, **type**: :ref:`string id <proto_egress_path_selection_string_id>` egress path value map

Per-escaper :ref:`string id <proto_egress_path_selection_string_id>` values for
this origin. Each map key is the target escaper name.

If this site does not set a path, the tenant user's
:ref:`egress_path_id_map <config_user_egress_path_id_map>` is used when
present. Visitor users are not consulted for origin path selection.

Example:

.. code-block:: yaml

   egress_path_id_map:
     direct-egress: hk-v4
     proxy-pool: corp-exit-2

.. _conf_site_egress_path_value_map:

egress_path_value_map
---------------------

**optional**, **type**: :ref:`json value <proto_egress_path_selection_json_value>` egress path value map

Per-escaper :ref:`json value <proto_egress_path_selection_json_value>` values
for this origin. Each map key is the target escaper name.

If this site does not set a path, the tenant user's
:ref:`egress_path_value_map <config_user_egress_path_value_map>` is used when
present.

Example:

.. code-block:: yaml

   egress_path_value_map:
     direct-float:
       ip: 203.0.113.11
       id: temp-egress
