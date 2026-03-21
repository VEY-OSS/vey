.. _log_task_udp_connect:

***********
Udp Connect
***********

The following keys are available in ``UdpConnect`` task logs:

tcp_server_addr
---------------

**required**, **type**: socket address string

The server address for the TCP control connection.

tcp_client_addr
---------------

**required**, **type**: socket address string

The client address for the TCP control connection.

udp_server_addr
---------------

**optional**, **type**: socket address string

The server address for the UDP data connection.

udp_client_addr
---------------

**optional**, **type**: socket address string

The client address for the UDP data connection.

upstream
--------

**required**, **type**: domain:port | socket address string

The target upstream requested by the client.

next_bind_ip
------------

**optional**, **type**: ip address string

The selected bind IP before the remote-side UDP socket is created.

Present only when bind-IP configuration is enabled on the corresponding
escaper.

next_bound_addr
---------------

**optional**, **type**: socket address string

The local address used for the remote UDP socket.

next_peer_addr
--------------

**optional**, **type**: socket address string

The peer address used for the remote UDP socket.

Depending on the escaper type, this may be either the final upstream or the
next proxy peer.

next_expire
-----------

**optional**, **type**: rfc3339 timestamp string with microseconds

The expected expiration time of the next peer.

Present only when the next escaper is dynamic and a remote peer has already
been selected.

c_rd_bytes
----------

**optional**, **type**: int

Total bytes received from the client.

c_rd_packets
------------

**optional**, **type**: int

Total packets received from the client.

c_wr_bytes
----------

**optional**, **type**: int

Total bytes sent to the client.

c_wr_packets
------------

**optional**, **type**: int

Total packets sent to the client.

r_rd_bytes
----------

**optional**, **type**: int

Total bytes received from the remote peer.

r_rd_packets
------------

**optional**, **type**: int

Total packets received from the remote peer.

r_wr_bytes
----------

**optional**, **type**: int

Total bytes sent to the remote peer.

r_wr_packets
------------

**optional**, **type**: int

Total packets sent to the remote peer.
