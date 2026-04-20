.. _protocol_helper_icap_imap:

=============
ICAP for IMAP
=============

``vey-proxy`` can use an ICAP ``REQMOD`` service for outgoing IMAP ``APPEND``
messages.

The mail message is converted into an HTTP/1.1 ``PUT`` request and sent to the
ICAP server. The ICAP server's response is then forwarded upstream.

The returned mail message must not change in size.

See also :doc:`headers` for the ICAP headers that may be added for all ICAP
adaptation requests.

The following protocol-specific header is added to the ICAP request headers:

- X-Transformed-From

  The value is **IMAP**.

The following headers are set on the HTTP ``PUT`` request:

- Content-Type

  The value is ``message/rfc822`` for the IMAP message payload.

- Content-Length

  The value is the exact size of the mail message in the IMAP ``APPEND``
  command.
  The ICAP server may modify the message body, but it must not change the total
  message size.
  The ICAP server should also return an updated ``Content-Length`` header in
  the HTTP request embedded in its ICAP response.

The body of the HTTP ``PUT`` request contains the mail message data.

Limitations
-----------

The ICAP server must not change the size of the mail message.
