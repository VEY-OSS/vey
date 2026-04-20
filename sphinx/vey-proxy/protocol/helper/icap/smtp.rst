.. _protocol_helper_icap_smtp:

=============
ICAP for SMTP
=============

``vey-proxy`` can use an ICAP ``REQMOD`` service for outgoing SMTP ``DATA``
messages.

The SMTP message is converted into an HTTP/1.1 ``PUT`` request and sent to the
ICAP server. The ICAP server's response is then converted back into an SMTP
message in the same format.

See also :doc:`headers` for the ICAP headers that may be added for all ICAP
adaptation requests.

The following protocol-specific header is added to the ICAP request headers:

- X-Transformed-From

  The value is **SMTP**.

The following headers are set on the HTTP ``PUT`` request:

- Content-Type

  The value is ``message/rfc822`` for the SMTP ``DATA`` payload.

- X-SMTP-From

  The value is the *reverse-path* from the SMTP ``MAIL`` command, containing
  the sender's mailbox address.

- X-SMTP-To

  The value is the *forward-path* from the SMTP ``RCPT`` command, containing
  the recipient mailbox address.
  This header appears multiple times when there is more than one recipient.

The body of the HTTP ``PUT`` request contains the SMTP message data.

Not Implemented
---------------

- BDAT message.
- BURL message.

These unsupported extensions are disabled by default in the auditor's
:ref:`smtp interception <conf_auditor_smtp_interception>` configuration.
