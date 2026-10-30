########################
``vey-iploc`` Reference
########################

``vey-iploc`` is an IP address location lookup service. ``vey-proxy`` sends
GeoIP queries to it over UDP or a Unix datagram socket. The query protocol is
the `IP locate helper protocol`_.

A typical deployment runs ``vey-iploc`` on the same host as ``vey-proxy``.
Each listen socket is a ``server`` entry. Country and ASN data are loaded from
VEY CSV databases named in ``geoip_db``.

The core configuration model is organized around a few main object types:

* ``server`` accepts lookup datagrams
* ``geoip_db`` names the country and ASN database files
* ``runtime``, ``worker``, ``stat``, and ``controller`` control how the daemon
  runs and how it reports operational state

This reference is organized into two sections:

* :doc:`configuration/index` documents all configuration objects and their
  relationships
* :doc:`metrics/index` documents the exported StatsD metrics

If you are setting up ``vey-iploc`` for the first time, start with the
configuration reference, then review the metrics pages for operational
visibility.

.. _IP locate helper protocol: https://vey.readthedocs.io/projects/proxy/en/latest/protocol/helper/ip_locate.html

.. toctree::
   :maxdepth: 1

   Configuration Reference <configuration/index>
   Metrics Definition <metrics/index>
