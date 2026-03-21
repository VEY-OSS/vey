.. _log_escape_udp_sendto:

*********
UdpSendto
*********

The following keys are available in ``UdpSendto`` escape logs:

next_expire
-----------

**optional**, **type**: rfc3339 timestamp string with microseconds

The expected expiration time of the next peer.

Present only when the next escaper is dynamic and a remote peer has already
been selected.

reason
------

**required**, **type**: enum string

The short error reason.
