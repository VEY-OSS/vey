.. _configuration_auth_user:

****
User
****

User configuration is a map. It defines how the user is authenticated, what
limits apply, and any user-specific behavior overrides.

name
----

**required**, **type**: :external+values:ref:`username <conf_value_username>`

Username.

.. _conf_auth_user_token:

token
-----

**required**, **type**: mix

Authentication token used for this user.

This config option will only be used by the following user groups:

* :ref:`basic <configuration_auth_user_group_basic>`

The token can take one of the following forms:

* null

  ``null`` disables password-token authentication.

  .. note:: This is different from not setting token value, which means forbid the user.

  .. versionadded:: 1.7.20

* str

  String in Unix ``crypt(5)`` format.

* map

  The ``type`` key selects the actual token subtype.

  * fast_hash

    Custom fast-hash format. It uses a salt plus one or more of ``md5``,
    ``sha1``, and ``blake3``. The hash is weak but fast.
    The values for ``salt``, ``md5``, ``sha1``, and ``blake3`` must be
    hex-encoded ASCII strings.

  * xcrypt_hash

    Requires a ``value`` key containing a valid ``crypt(5)`` string.

The currently supported crypt(5) methods are: md5, sha256, sha512.

match_by_facts
--------------

**optional**, **type**: :external+values:ref:`facts_match_value <conf_value_facts_match_value>` | seq

Authentication facts that match this user.

This config option will only be used by the following user groups:

* :ref:`facts <configuration_auth_user_group_facts>`

**default**: not set

.. versionadded:: 1.13.0

expire
------

**optional**, **type**: :external+values:ref:`rfc3339 datetime str <conf_value_rfc3339_datetime_str>`

Time at which the user is considered expired. The check interval is controlled
by
:ref:`refresh interval <conf_auth_user_group_refresh_interval>` in group config.

**default**: not set

block_and_delay
---------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Blocks the user and delays the error response by the configured duration.

The response code for blocked user will be forbidden instead of auth failed.

**default**: not set

ingress_network_filter
----------------------

**optional**, **type**: :external+values:ref:`ingress network acl rule <conf_value_ingress_network_acl_rule>`

Ingress network filter for clients.

If a server is chained behind a PROXY Protocol server, the client address used
here is the one carried in the PROXY Protocol message.

This ACL is checked before the anonymous-auth path is considered, so the client
receives an authentication failure and anonymous-user forbidden metrics are not
incremented.

**default**: not set

.. versionadded:: 1.7.20

proxy_request_filter
--------------------

**optional**, **type**: :external+values:ref:`proxy request acl rule <conf_value_proxy_request_acl_rule>`

Proxy request types this user is allowed to use.

**default**: not set

dst_host_filter_set
-------------------

**optional**, **type**: :external+values:ref:`dst host acl rule set <conf_value_dst_host_acl_rule_set>`

Destination-host filter for each request. It does not apply to UDP ASSOCIATE
tasks.

**default**: not set

dst_port_filter
---------------

**optional**, **type**: :external+values:ref:`exact port acl rule <conf_value_exact_port_acl_rule>`

Destination-port filter for each request. It does not apply to UDP ASSOCIATE
tasks.

**default**: not set

http_user_agent_filter
----------------------

**optional**, **type**: :external+values:ref:`user agent acl rule <conf_value_user_agent_acl_rule>`

Filter for the HTTP ``User-Agent`` header.

.. note:: This only applies to layer-7 http traffic, including http forward and https forward.

**default**: not set

tcp_connect
-----------

**optional**, **type**: :external+values:ref:`tcp connect <conf_value_tcp_connect>`

User-level TCP connect parameters. These apply to *direct* escapers and are
further constrained by escaper-level settings.

**default**: not set

tcp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Per-TCP-socket speed limit.

**default**: no limit

tcp_conn_speed_limit
--------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use tcp_sock_speed_limit instead

tcp_conn_limit
--------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use tcp_sock_speed_limit instead

udp_sock_speed_limit
---------------------

**optional**, **type**: :external+values:ref:`udp socket speed limit <conf_value_udp_sock_speed_limit>`

Per-UDP-socket speed limit.

**default**: no limit

udp_relay_speed_limit
---------------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use udp_sock_speed_limit instead

udp_relay_limit
---------------

**deprecated**

.. versionchanged:: 1.11.8 deprecated, use udp_sock_speed_limit instead

tcp_all_upload_speed_limit
--------------------------

**optional**, **type**: :external+values:ref:`global stream speed limit <conf_value_global_stream_speed_limit>`

Process-level upload speed limit for all client-side TCP connections.

This will only count in the data that will be forwarded.

**default**: no limit

.. versionadded:: 1.9.6

tcp_all_download_speed_limit
----------------------------

**optional**, **type**: :external+values:ref:`global stream speed limit <conf_value_global_stream_speed_limit>`

Process-level download speed limit for all client-side TCP connections.

This will only count in the data received from upstream.

**default**: no limit

.. versionadded:: 1.9.6

udp_all_upload_speed_limit
--------------------------

**optional**, **type**: :external+values:ref:`global datagram speed limit <conf_value_global_datagram_speed_limit>`

Process-level upload speed limit for all client-side UDP connections.

This will only count in the data that will be forwarded.

**default**: no limit

.. versionadded:: 1.9.6

udp_all_download_speed_limit
----------------------------

**optional**, **type**: :external+values:ref:`global datagram speed limit <conf_value_global_datagram_speed_limit>`

Process-level download speed limit for all client-side UDP connections.

This will only count in the data received from upstream.

**default**: no limit

.. versionadded:: 1.9.6

tcp_remote_keepalive
--------------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

TCP keepalive configuration for the remote TCP socket.

The tcp keepalive set in user config will only be taken into account in Direct type escapers.

**default**: no keepalive set

tcp_remote_misc_opts
--------------------

**optional**, **type**: :external+values:ref:`tcp misc sock opts <conf_value_tcp_misc_sock_opts>`

Miscellaneous TCP socket options for the remote TCP socket.

The user level TOS and Mark config will overwrite the one set at escaper level.
Other fields will be limited to the smaller ones.

**default**: not set

udp_remote_misc_opts
--------------------

**optional**, **type**: :external+values:ref:`udp misc sock opts <conf_value_udp_misc_sock_opts>`

Miscellaneous UDP socket options for the remote UDP socket.

The user level TOS and Mark config will overwrite the one set at escaper level.
Other fields will be limited to the smaller ones.

**default**: not set

tcp_client_misc_opts
--------------------

**optional**, **type**: :external+values:ref:`tcp misc sock opts <conf_value_tcp_misc_sock_opts>`

Miscellaneous TCP socket options for the client TCP socket before the task
enters the connection stage.

The user level TOS and Mark config will overwrite the one set at escaper level.
Other fields will be limited to the smaller ones.

**default**: not set

udp_client_misc_opts
--------------------

**optional**, **type**: :external+values:ref:`udp misc sock opts <conf_value_udp_misc_sock_opts>`

Miscellaneous UDP socket options for the client UDP socket.

The user level TOS and Mark config will overwrite the one set at server level.
Other fields will be limited to the smaller ones.

**default**: not set

http_upstream_keepalive
-----------------------

**optional**, **type**: :external+values:ref:`http keepalive <conf_value_http_keepalive>`

HTTP keepalive configuration at the user level.

**default**: set with default value

.. _conf_user_http_rsp_header_recv_timeout:

http_rsp_header_recv_timeout
----------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Custom HTTP response-header receive timeout for this user.

This will overwrite:

- http proxy server :ref:`rsp_header_recv_timeout <conf_server_http_proxy_rsp_header_recv_timeout>`
- auditor :ref:`h1 interception <conf_auditor_h1_interception>`
- auditor :ref:`h2 interception <conf_auditor_h1_interception>`

This will be overwritten by:

- user-site :ref:`http_rsp_header_recv_timeout <conf_user_site_http_rsp_header_recv_timeout>`

**default**: not set

.. versionadded:: 1.9.0

tcp_conn_rate_limit
-------------------

**deprecated**, **alias**: tcp_conn_limit_quota

.. versionchanged:: 1.13.0 deprecated, use `connection_rate_limit` instead

connection_rate_limit
---------------------

**optional**, **type**: :external+values:ref:`rate limit quota <conf_value_rate_limit_quota>`

Rate limit for new client-side connections.

The same connection used for different users will be counted for each of them.

**default**: no limit

.. versionadded:: 1.13.0

request_rate_limit
------------------

**optional**, **type**: :external+values:ref:`rate limit quota <conf_value_rate_limit_quota>`

Rate limit for requests.

**default**: no limit, **alias**: request_limit_quota

request_max_alive
-----------------

**optional**, **type**: usize, **alias**: request_alive_max

Maximum number of active requests for this user.

Even if not set, the max alive requests should not be more than usize::MAX.

**default**: no limit

resolve_strategy
----------------

**optional**, **type**: :external+values:ref:`resolve strategy <conf_value_resolve_strategy>`

Custom resolve strategy for this user, constrained by the strategy allowed by
the escaper.
Not all escapers support this, see the documentation for each escaper for more info.

**default**: no custom resolve strategy is set

resolve_redirection
-------------------

**optional**, **type**: :external+values:ref:`resolve redirection <conf_value_resolve_redirection>`

DNS redirection rules for this user.

**default**: not set

log_rate_limit
--------------

**optional**, **type**: :external+values:ref:`rate limit quota <conf_value_rate_limit_quota>`

Rate limit for log requests.

**default**: no limit, **alias**: log_limit_quota

.. _config_user_log_uri_max_chars:

log_uri_max_chars
-----------------

**optional**, **type**: usize

Maximum number of URI characters recorded in logs.

If set, this will override the one set in server level.

If not set, the one in server level will take effect.

Passwords embedded in URIs are replaced with ``xyz`` before logging.

**default**: not set

task_idle_max_count
-------------------

**optional**, **type**: usize

The task is closed once the idle check reports ``IDLE`` this many times.

This will overwrite the one set at server side,
see :ref:`server task_idle_max_count <conf_server_common_task_idle_max_count>`.

The idle-check interval can only be configured at the server level,
see :ref:`server task_idle_check_interval <conf_server_common_task_idle_check_interval>`.

**default**: not set

.. versionchanged:: 1.11.3 change default from 1 to not set

socks_use_udp_associate
-----------------------

**optional**, **type**: bool

Controls whether SOCKS UDP ASSOCIATE is used instead of the simplified UDP
CONNECT mode.

**default**: false

audit
-----

**optional**, **type**: :ref:`user audit <configuration_auth_user_audit>`

Audit configuration for this user.

**default**: set with default values

explicit_sites
--------------

**optional**, **type**: seq of :ref:`user site <configuration_auth_user_site>`

Explicit per-site configuration for this user.

.. _config_user_egress_path_id_map:

egress_path_id_map
------------------

**optional**, **type**: :ref:`string id <proto_egress_path_selection_string_id>` egress path value map

ID-based egress path selection for this user.

.. versionadded:: 1.9.2

.. _config_user_egress_path_value_map:

egress_path_value_map
---------------------

**optional**, **type**: :ref:`json value <proto_egress_path_selection_json_value>` egress path value map

JSON-value-based egress path selection for this user.

.. versionadded:: 1.9.2
