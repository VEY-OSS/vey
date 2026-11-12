.. _configuration_site:

****
Site
****

**optional**, **type**: :external+values:ref:`hybrid map <conf_value_hybrid_map>`, **alias**: sites

Predefined site certificates. Each map is one site.

A matching certificate is returned as-is. The issuer CA chain is not appended.
A request with no site id, no matching site, or no certificate for that usage
is signed by the :doc:`issuer <issuer>`.

``reload`` re-reads ``site`` from the main config and replaces the running set
after every map loads. Omitting ``site`` clears the running set. A map loaded
from a file records that file. ``reload-site <SITE-ID>`` re-reads only that
file, and applies the result only when the loaded ``id`` is ``SITE-ID``. A
site with no recorded file cannot be reloaded this way. ``reload-issuer``
does not read these maps.

.. versionadded:: 0.10.0

Each map must set ``id``. At least one of ``tls_server``, ``tlcp_server_sign``,
and ``tlcp_server_enc`` must be set. Each value is a certificate pair, so both
its certificate and private key are required.

id
==

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Site id sent by ``vey-proxy`` with a certificate request.

ttl
===

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

TTL reported with a predefined certificate from this site. A number is a count
of seconds.

**default**: 86400

tls_server
==========

**optional**, **type**: :external+values:ref:`tls cert pair <conf_value_tls_cert_pair>`

TLS certificate and private key. Used for ``tls_server`` and
``tls_server_tongsuo`` requests.

tlcp_server_sign
================

**optional**, **type**: :external+values:ref:`tls cert pair <conf_value_tls_cert_pair>`

TLCP signature certificate and private key. Used for
``tlcp_server_signature`` requests.

tlcp_server_enc
===============

**optional**, **type**: :external+values:ref:`tls cert pair <conf_value_tls_cert_pair>`

TLCP encryption certificate and private key. Used for
``tlcp_server_encryption`` requests.
