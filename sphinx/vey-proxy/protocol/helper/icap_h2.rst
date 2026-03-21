.. _protocol_helper_icap_h2:

===========
ICAP for H2
===========

``vey-proxy`` can use ICAP ``REQMOD`` and ``RESPMOD`` services for HTTP/2
requests and responses.

HTTP/2 requests and responses are first transformed into HTTP/1.1 messages
before being sent to the ICAP server. The ICAP server's response is then
converted back into HTTP/2.

The following headers are added to the ICAP request headers:

- X-Transformed-From

  The value is **HTTP/2.0**.

- X-HTTP-Upgrade

  The value is the protocol value from the Extended CONNECT request.
