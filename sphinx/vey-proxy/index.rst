vey-proxy Documentation
=======================

``vey-proxy`` is a programmable proxy server that combines multiple server
entry points, flexible egress routing, pluggable authentication, and detailed
observability. It supports direct forwarding, upstream proxy chaining,
transparent proxy deployments, protocol-aware helper services, and a large set
of runtime metrics and structured logs.

This documentation is organized by operational concern so you can move quickly
from high-level understanding to concrete configuration details:

* Use the configuration reference to understand how to define servers,
  escapers, resolvers, auth backends, loggers, and shared value types.
* Use the protocol reference to understand client-side headers, helper-service
  protocols, and deployment-related setup topics.
* Use the metrics and log references when integrating monitoring, dashboards,
  alerting, or downstream log pipelines.

The sections below are the main entry points.

Documentation Map
=================

.. toctree::
   :maxdepth: 1

   Configuration Reference <configuration/index>
   Protocol Details <protocol/index>
   Metrics Definition <metrics/index>
   Log Format <log/index>

What You Will Find
==================

Configuration Reference
-----------------------

The configuration reference documents the static configuration model used by
``vey-proxy``. It covers the top-level runtime layout and the concrete
configuration types for:

* servers that accept client traffic
* escapers that decide how outbound traffic is forwarded
* resolvers that control name resolution behavior
* auth components that define users, groups, and auth sources
* logging, metrics, and shared value objects reused across the config tree

Protocol Details
----------------

The protocol section describes interfaces around ``vey-proxy`` rather than the
configuration syntax itself. This includes client-facing protocol extensions,
helper-service protocols used by external components, and setup guides for
deployment scenarios such as transparent proxying and packet capture.

Metrics Definition
------------------

The metrics section explains the exported counters, gauges, and tags used by
``vey-proxy``. It is the reference to use when building dashboards, tuning
StatsD ingestion, or interpreting per-server, per-escaper, and per-user
traffic statistics.

Log Format
----------

The log section documents the structured log records emitted by different
subsystems, including request task logs, escape logs, and resolver logs. Use
this section when integrating with log collectors or when you need exact field
definitions for analysis pipelines.
