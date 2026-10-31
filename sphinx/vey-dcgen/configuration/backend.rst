.. _configuration_backend:

*******
Backend
*******

**required**, **type**: map

The certificate authority used to issue certificates. This section is read
only at startup.

ca_certificate
==============

**required**, **type**: file path

CA certificate chain, in a PEM file. The path may be absolute or relative to
the config directory. The first certificate signs issued certificates. The
encoded chain is appended to each issued certificate unless
``no_append_ca_cert`` is true.

ca_private_key
==============

**required**, **type**: file path

Private key of the signing certificate, in a PEM file. The path may be
absolute or relative to the config directory.

no_append_ca_cert
=================

**optional**, **type**: bool

Do not append the CA certificate chain to issued certificates. Default is
false.

keep_serial
===========

**optional**, **type**: bool

Keep the serial number when mimicking an upstream certificate. Default is
false.

max_ttl
=======

**optional**, **type**: i32

Upper bound, in seconds, on the TTL reported to ``vey-proxy``. Values below
300 are raised to 300. Default is 86400.

duration_stats
==============

**optional**, **type**: :external+values:ref:`histogram metrics <conf_value_histogram_metrics>`

Histogram of the time from receiving a request datagram to writing the
response. ``duration_metrics`` is an alias.
