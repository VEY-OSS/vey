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

Each map must set ``id``. At least one of the TLS, TLCP signature, and TLCP
encryption pairs must be set. A pair is used only when both its certificate
and private key are set.

id
==

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Site id sent by ``vey-proxy`` with a certificate request.

ttl
===

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

TTL reported with a predefined certificate from this site. A number is a count
of seconds.

An already expired certificate is not returned. When this value is longer than
the time left before the leaf expires, the reported TTL is the remaining time.

**default**: 86400

certificate
===========

**optional**, **type**: :external+values:ref:`tls certificates <conf_value_tls_certificates>`, **alias**: cert

TLS certificate chain in PEM. The first certificate is the leaf. Used for
``tls_server`` and ``tls_server_tongsuo`` requests.

private_key
===========

**optional**, **type**: :external+values:ref:`tls private_key <conf_value_tls_private_key>`, **alias**: key

Private key of the TLS leaf certificate.

sign_certificate
================

**optional**, **type**: :external+values:ref:`tls certificates <conf_value_tls_certificates>`, **alias**: sign_cert

TLCP signature certificate chain in PEM. Used for ``tlcp_server_signature``
requests.

sign_private_key
================

**optional**, **type**: :external+values:ref:`tls private_key <conf_value_tls_private_key>`, **alias**: sign_key

Private key of the TLCP signature certificate.

enc_certificate
===============

**optional**, **type**: :external+values:ref:`tls certificates <conf_value_tls_certificates>`, **alias**: enc_cert

TLCP encryption certificate chain in PEM. Used for ``tlcp_server_encryption``
requests.

enc_private_key
===============

**optional**, **type**: :external+values:ref:`tls private_key <conf_value_tls_private_key>`, **alias**: enc_key

Private key of the TLCP encryption certificate.
