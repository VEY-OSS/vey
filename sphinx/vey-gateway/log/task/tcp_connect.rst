.. _log_task_tcp_connect:

***********
Tcp Connect
***********

The following fields are present in ``TcpConnect`` task logs:

server_addr
-----------

**required**, **type**: socket address string

The listening address of the server.

client_addr
-----------

**required**, **type**: socket address string

The client address.

c_rd_bytes
----------

**optional**, **type**: int

The number of bytes received from the client.

c_wr_bytes
----------

**optional**, **type**: int

The number of bytes sent to the client.

r_rd_bytes
----------

**optional**, **type**: int

The number of bytes received from the remote peer.

r_wr_bytes
----------

**optional**, **type**: int

The number of bytes sent to the remote peer.
