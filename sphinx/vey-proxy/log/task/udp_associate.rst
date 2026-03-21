.. _log_task_udp_associate:

*************
Udp Associate
*************

The following keys are available in ``UdpAssociate`` task logs:

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

initial_peer
------------

**optional**, **type**: socket address string

The target peer address from the first UDP packet.

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
