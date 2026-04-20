.. _protocol_helper:

###############
Helper Protocol
###############

The following helper protocols are used when ``vey-proxy`` integrates with
external services.

.. toctree::
   :hidden:

   route_query
   cert_generator
   ip_locate
   icap/index
   stream_detour

- route_query

  Used by the ``route_query`` escaper to query an external routing service. See
  :doc:`route_query`.

- cert_generator

  Used by the auditor during TLS interception to request generated
  certificates. See :doc:`cert_generator`.

- ip_locate

  Used by the ``route_geoip`` escaper to look up IP location data. See
  :doc:`ip_locate`.

- icap

  Describes the ICAP helper protocol and the protocol-specific integrations for
  HTTP/1.x, HTTP/2, IMAP, and SMTP. See :doc:`icap/index`.

- stream_detour

  Used by the auditor to send client-side and remote-side streams to an
  external interception server. See :doc:`stream_detour`.
