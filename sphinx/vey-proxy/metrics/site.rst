.. _metrics_site:

############
Site Metrics
############

Site metrics describe application-layer activity for each origin site in a
:ref:`site group <configuration_site_group>`. They are distinct from
:ref:`user-site metrics <metrics_user_site>`: ``site.*`` is keyed by the
origin, while ``user.site.*`` is keyed by the authenticated user plus a
user-site ID.

Metric names use a fixed ``site.`` prefix. The site identity is reported as
tags, so values for the same metric name can be aggregated across sites.

The following tags are present on all site metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* site_group

  The :ref:`site group <conf_site_group_name>` that contains the site.

* site

  The :ref:`site ID <conf_site_id>`.

* user

  The :ref:`owner <conf_site_owner>` configured on the site. ``-`` when the
  site has no owner. This is the configured name, not a lookup result.

* user_group

  The site group's :ref:`tenant_user_group <conf_site_group_tenant_user_group>`.
  ``-`` when that key is unset. This is the configured name, not a lookup
  result.

Request
=======

The following tags are set on metrics in this section:

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

The following tag is also set on ``site.connection.*`` metrics:

* :ref:`connection <metrics_tag_connection>`

The following tag is also set on ``site.request.*`` metrics:

* :ref:`request <metrics_tag_request>`

The metric names are:

* site.connection.total

  **type**: count

  Number of client connections that matched this site.

* site.request.total

  **type**: count

  Total requests that matched this site. This value may be greater than
  ``site.connection.total`` because some protocols can reuse a connection for
  multiple requests.

* site.request.alive

  **type**: gauge

  Number of currently active requests for this site.

* site.request.ready

  **type**: count

  Total tasks for this site that reached the *ready* stage. The remote
  connection may be new or a reused keepalive connection.

* site.request.reuse

  **type**: count

  Total attempts to reuse an existing remote keepalive connection.
  Reuse attempts may still fail.

* site.request.renew

  **type**: count

  Total failed attempts to reuse an existing remote keepalive connection. After
  a recoverable reuse failure, a new connection is created and the request is
  retried.

* site.l7.connection.alive

  **type**: gauge

  Number of currently active layer-7 proxy connections for this site.

Forbidden
=========

The following tags are set on metrics in this section:

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

These counters increment only when an **origin-site** rule rejects the request.
Tenant (owner) rules increment that owner's :ref:`user.forbidden.*
<metrics_user>` counters. Visitor rules increment the visitor's
``user.forbidden.*`` counters. A rejection is never counted on both a site
counter and a user counter.

The metric names are:

* site.forbidden.crypto_error

  **type**: count

  Number of rejected requests caused by an internal crypto error attributed to
  this site.

* site.forbidden.auth_failed

  **type**: count

  Number of rejected requests caused by authentication failure attributed to
  this site.

* site.forbidden.user_expired

  **type**: count

  Number of rejected requests caused by an expired identity attributed to this
  site.

* site.forbidden.user_blocked

  **type**: count

  Number of rejected requests caused by a blocked identity attributed to this
  site.

* site.forbidden.fully_loaded

  **type**: count

  Number of requests rejected because the site
  :ref:`request_max_alive <conf_site_request_max_alive>` limit was reached.

* site.forbidden.rate_limited

  **type**: count

  Number of requests rejected because the site
  :ref:`request_rate_limit <conf_site_request_rate_limit>` was exceeded.

* site.forbidden.proto_banned

  **type**: count

  Number of requests rejected because a site-level protocol rule banned the
  request. Tenant and visitor ``audit.prohibit_*`` rules increment
  ``user.forbidden.proto_banned`` instead.

* site.forbidden.src_blocked

  **type**: count

  Number of requests rejected because a site-level source ACL blocked the
  client.

* site.forbidden.dest_denied

  **type**: count

  Number of requests rejected because a site-level destination rule forbade the
  upstream.

* site.forbidden.ip_blocked

  **type**: count

  Number of requests rejected because a site-level resolved-IP rule blocked the
  address.

* site.forbidden.log_skipped

  **type**: count

  Number of requests for which logging was skipped by a site-level rule.

* site.forbidden.ua_blocked

  **type**: count

  Number of layer-7 HTTP requests blocked by a site-level User-Agent rule.

Traffic
=======

The following tags are set on metrics in this section:

* :ref:`request <metrics_tag_request>`

* server

  The server name that received the request.

Any extra tags configured on the server are also included.

These I/O metrics include application-layer traffic only.

The metric names are:

* site.traffic.in.bytes

  **type**: count

  Total bytes received from the client.

* site.traffic.in.packets

  **type**: count

  Total datagram packets received from the client.
  This metric is not available for stream-oriented transports.

* site.traffic.out.bytes

  **type**: count

  Total bytes sent to the client.

* site.traffic.out.packets

  **type**: count

  Total datagram packets sent to the client.
  This metric is not available for stream-oriented transports.

Upstream Traffic
================

The following tags are set on metrics in this section:

* :ref:`transport <metrics_tag_transport>`

* escaper

  The escaper name that handled the upstream side of the request.

Any extra tags configured on the escaper are also included.

These I/O metrics include application-layer traffic only.

The metric names are:

* site.upstream.traffic.in.bytes

  **type**: count

  Total bytes received from the upstream side.

* site.upstream.traffic.in.packets

  **type**: count

  Total datagram packets received from the upstream side.
  This metric is not available for stream-oriented transports.

* site.upstream.traffic.out.bytes

  **type**: count

  Total bytes sent to the upstream side.

* site.upstream.traffic.out.packets

  **type**: count

  Total datagram packets sent to the upstream side.
  This metric is not available for stream-oriented transports.
