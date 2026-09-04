.. _configuration_server_tls_proxy:

tls_proxy
=========

This server terminates TLS on the frontend, selects a site from a
:ref:`site group <configuration_site_group>` by SNI, and copies the inner
TCP stream to that site's upstream.

Unlike :ref:`tls_stream <configuration_server_tls_stream>`, certificates and
upstreams live on the site, not on this server. Unlike
:ref:`http_expose <configuration_server_http_rproxy>`, this server does not
parse HTTP; it only copies bytes after the handshake.

There is no visitor ``user_group``. Tenant identity comes from
``site.owner`` and the site group's
:ref:`tenant_user_group <conf_site_group_tenant_user_group>`.

Sites without :ref:`tls_server <conf_site_tls_server>` are skipped. This
server has no fallback certificate and no default upstream.

The following common keys are supported:

* :ref:`escaper <conf_server_common_escaper>`
* :ref:`auditor <conf_server_common_auditor>`

  Optional. When unset, the inner stream is copied without protocol
  inspection. When set, the inner stream is inspected
  (``dpi_protocol`` on the site is used as a hint). HTTP discovered by
  inspection can continue through that auditor's ICAP services.

* :ref:`shared_logger <conf_server_common_shared_logger>`
* :ref:`listen_in_worker <conf_server_common_listen_in_worker>`
* :ref:`tls ticketer <conf_server_common_tls_ticketer>`
* :ref:`tcp_sock_speed_limit <conf_server_common_tcp_sock_speed_limit>`
* :ref:`ingress_network_filter <conf_server_common_ingress_network_filter>`
* :ref:`tcp_copy_buffer_size <conf_server_common_tcp_copy_buffer_size>`
* :ref:`tcp_copy_yield_size <conf_server_common_tcp_copy_yield_size>`
* :ref:`tcp_misc_opts <conf_server_common_tcp_misc_opts>`
* :ref:`task_idle_check_interval <conf_server_common_task_idle_check_interval>`
* :ref:`task_idle_max_count <conf_server_common_task_idle_max_count>`
* :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
* :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
* :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
* :ref:`extra_metrics_tags <conf_server_common_extra_metrics_tags>`

listen
------

**optional**, **type**: :external+values:ref:`tcp listen <conf_value_tcp_listen>`

Listening configuration for this server.

The instance count setting will be ignored if *listen_in_worker* is correctly enabled.

**default**: not set

site_group
----------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Name of the :ref:`site group <configuration_site_group>` that provides SNI
matching, ingress certificates, and per-site upstream settings.

This server reads only the site root. The site ``http:`` subtree is ignored.
Sites in the group that omit ``tls_server`` are not served here.

If the referenced group does not exist, an empty group is used and no site
matches.

**default**: not set

client_hello_recv_timeout
-------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for receiving the complete TLS ClientHello message used to pick
the site certificate from SNI.

**default**: 1s
