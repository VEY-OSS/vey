#################
VEY Documentation
#################

VEY is an open source project for building enterprise-oriented proxy, gateway,
and supporting network services in Rust.

The project includes several standalone daemons and tools that share common
design goals:

* reusable network and policy components
* operational visibility through logs and metrics
* deployment-friendly configuration and packaging
* modular service composition across multiple applications

For source code, release artifacts, and the top-level project README, see the
`code repository`_.

Applications
============

Follow the links below for the documentation of each application:

* `vey-proxy`_

  A feature-rich general-purpose proxy daemon with forward proxy, transparent
  proxy, stream proxy, inspection, and policy-control capabilities.

* `vey-statsd`_

  A StatsD-compatible metrics ingestion, aggregation, and forwarding service.

* `vey-gateway`_

  A work-in-progress general-purpose reverse proxy and gateway daemon.
  The current documented implementation focuses on TLS- and keyless-related
  traffic handling.

* `vey-keyless`_

  A server implementation of the Cloudflare Keyless SSL protocol.

Shared Reference
================

Some configuration value types are shared across multiple applications.
Those are documented in the common `vey-values`_ reference.

.. _code repository: https://github.com/VEY-OSS/vey

.. _vey-proxy: /projects/proxy/en/latest/
.. _vey-statsd: /projects/statsd/en/latest/
.. _vey-gateway: /projects/gateway/en/latest/
.. _vey-keyless: /projects/keyless/en/latest/
.. _vey-values: /projects/values/en/latest/
