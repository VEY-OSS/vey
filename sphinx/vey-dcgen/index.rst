########################
``vey-dcgen`` Reference
########################

``vey-dcgen`` issues per-host certificates for ``vey-proxy`` TLS interception.
``vey-proxy`` sends certificate requests to it over UDP or a Unix datagram
socket. The request protocol is the `cert generator helper protocol`_.

A typical deployment runs ``vey-dcgen`` on the same host as ``vey-proxy``.
Each listen socket is a ``server`` entry. Certificates are signed by the
certificate authority named in ``issuer``.

The core configuration model is organized around a few main object types:

* ``server`` accepts certificate-request datagrams
* ``issuer`` names the CA certificate and private key
* ``runtime``, ``worker``, ``stat``, and ``controller`` control how the daemon
  runs and how it reports operational state

This reference is organized into two sections:

* :doc:`configuration/index` documents all configuration objects and their
  relationships
* :doc:`metrics/index` documents the exported StatsD metrics

If you are setting up ``vey-dcgen`` for the first time, start with the
configuration reference, then review the metrics pages for operational
visibility.

.. _cert generator helper protocol: https://vey.readthedocs.io/projects/proxy/en/latest/protocol/helper/cert_generator.html

.. toctree::
   :maxdepth: 1

   Configuration Reference <configuration/index>
   Metrics Definition <metrics/index>
