.. _log_escape_tcp_connect:

**********
TcpConnect
**********

The following keys are available in ``TcpConnect`` escape logs:

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

tcp_connect_tries
-----------------

**optional**, **type**: int

Number of connection attempts made to the remote peer.

tcp_connect_spend
-----------------

**optional**, **type**: time duration string

Total time spent attempting to connect to the remote peer, including all
retries.

reason
------

**required**, **type**: enum string

The short error reason.

override_peer
-------------

**optional**, **type**: domain:port | socket address string

The overridden peer or upstream address selected by egress path selection.

.. versionadded:: 1.13.0
