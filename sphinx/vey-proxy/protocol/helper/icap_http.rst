.. _protocol_helper_icap_http:

=============
ICAP for HTTP
=============

``vey-proxy`` can use ICAP ``REQMOD`` and ``RESPMOD`` services for HTTP/1.x
requests and responses.

The following header is added to the ICAP request headers:

- X-HTTP-Upgrade

  If the original HTTP request contains an ``Upgrade`` header, its value is
  copied into ``X-HTTP-Upgrade`` in the ICAP request.
