.. _protocol_client_http_custom_codes:

#################
Http Custom Codes
#################

``vey-proxy`` uses the following non-standard HTTP response codes in addition to
the standard ones:

* 521 WEB_SERVER_IS_DOWN

  The upstream server or the next proxy peer refused the connection or reset it
  after the connection attempt.

* 522 CONNECTION_TIMED_OUT

  The connection attempt to the upstream server or next proxy peer timed out.

* 523 ORIGIN_IS_UNREACHABLE

  A network error such as ``network unreachable`` or ``host unreachable``
  occurred while connecting to the upstream server or next proxy peer.

* 525 SSL_HANDSHAKE_FAILED

  The TLS handshake with the upstream server failed.

  .. note::

    If the TLS handshake fails when connecting to the next proxy peer rather
    than the final upstream server, ``vey-proxy`` returns an internal server
    error instead. This distinction exists because proxy peers often use a
    separate TLS client configuration.

* 530 ORIGIN_DNS_ERROR

  DNS resolution failed for the upstream server or next proxy peer.
