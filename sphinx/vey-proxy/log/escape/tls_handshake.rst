.. _log_escape_tls_handshake:

************
TlsHandshake
************

The following keys are available in ``TlsHandshake`` escape logs:

next_bind_ip
------------

**optional**, **type**: ip address string

The selected bind IP before the connection to the remote peer is attempted.

Present only when bind-IP configuration is enabled on the corresponding
escaper.

next_expire
-----------

**optional**, **type**: rfc3339 timestamp string with microseconds

The expected expiration time of the next peer.

Present only when the next escaper is dynamic and a remote peer has already
been selected.

tls_name
--------

**required**, **type**: domain name or ip string

The TLS name used to verify the remote peer certificate.

tls_peer
--------

**required**, **type**: domain:port | socket address string

The remote peer with which the TLS session is established.

tls_application
---------------

**required**, **type**: enum string

The application protocol intended to run inside the TLS channel.

The values are:

* HttpForward

  The user sent an ``HttpsForward`` request, so ``vey-proxy`` had to establish
  a TLS channel.

* HttpProxy

  The next peer is an HTTPS proxy.
