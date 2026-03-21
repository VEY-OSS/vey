.. _log_task_tcp_connect:

***********
Tcp Connect
***********

The following keys are available in ``TcpConnect`` task logs:

server_addr
-----------

**required**, **type**: socket address string

The listening address of the server.

client_addr
-----------

**required**, **type**: socket address string

The client address.

upstream
--------

**required**, **type**: domain:port | socket address string

The target upstream requested by the client.

next_bind_ip
------------

**optional**, **type**: ip address string

The selected bind IP before the connection to the remote peer is attempted.

Present only when bind-IP configuration is enabled on the corresponding
escaper.

next_bound_addr
---------------

**optional**, **type**: socket address string

The local address used for the remote connection.

Present only after a connection to the remote peer has been established.

next_peer_addr
--------------

**optional**, **type**: socket address string

The peer address used for the remote connection.

Depending on the escaper type, this may be either the final upstream or the
next proxy peer.

Present only after the next peer address has been selected.

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

c_rd_bytes
----------

**optional**, **type**: int

Total bytes received from the client.

c_wr_bytes
----------

**optional**, **type**: int

Total bytes sent to the client.

r_rd_bytes
----------

**optional**, **type**: int

Total bytes received from the remote peer.

r_wr_bytes
----------

**optional**, **type**: int

Total bytes sent to the remote peer.
