.. _configuration_auth_user_site:

*********
User Site
*********

User-site configuration is a map. It defines how a site is matched, whether
site-level metrics are emitted, and any other site-specific overrides.

.. _conf_auth_user_site_id:

id
--

**required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

Each site must have an ID. If site metrics are enabled, this ID is used in the
metric name.

exact_match
-----------

**optional**, **type**: :ref:`host <conf_value_host>`

Exact domain or target IP address to match in the user request.

.. note:: the value should be different within all sites config of the current user.

child_match
-----------

**optional**, **type**: :ref:`domain <conf_value_domain>`

Parent domain to match. Any child domain under it also matches.

.. note:: the value should be different within all sites config of the current user.

subnet_match
------------

**optional**, **type**: :ref:`ip network str <conf_value_ip_network_str>`

Network to match when the user request target is an IP address.

.. note:: the value should be different within all sites config of the current user.

emit_stats
----------

**optional**, **type**: bool

Controls whether site-level metrics are emitted for this site.

See :ref:`user site metrics <metrics_user_site>` for the definition of metrics.

**default**: false

duration_stats
--------------

**optional**, **type**: :ref:`histogram metrics <conf_value_histogram_metrics>`

Histogram-metric configuration for site-level duration statistics.

**default**: set with default value

.. versionadded:: 1.7.32

resolve_strategy
----------------

**optional**, **type**: :ref:`resolve strategy <conf_value_resolve_strategy>`

Custom resolve strategy at the user-site level. It overrides the user-level
strategy, but must still remain within the limits allowed by the escaper.
Not all escapers support this, see the documentation for each escaper for more info.

**default**: no custom resolve strategy is set

.. versionadded:: 1.7.10

tls_client
----------

**optional**, **type**: :ref:`tls client <conf_value_openssl_tls_client_config>`

TLS client configuration used for the upstream-side handshake during TLS
interception.

This will overwrite:

- auditor `tls_interception_client <conf_auditor_tls_interception_client>` if tls interception is enabled
- http_proxy server `tls_client <conf_server_http_proxy_tls_client>` if https forward is enabled

**default**: not set

.. versionadded:: 1.9.0

.. _conf_user_site_http_rsp_header_recv_timeout:

http_rsp_header_recv_timeout
----------------------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Custom HTTP response-header receive timeout for this site.

This will set and overwrite:

- User :ref:`http_rsp_header_recv_timeout <conf_user_http_rsp_header_recv_timeout>`

**default**: not set

.. versionadded:: 1.9.0
