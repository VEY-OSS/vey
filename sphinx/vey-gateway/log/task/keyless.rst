.. _log_task_keyless:

*******
Keyless
*******

The following fields are present in ``Keyless`` task logs:

server_addr
-----------

**required**, **type**: socket address string

The listening address of the server.

client_addr
-----------

**required**, **type**: socket address string

The client address.

req_total
---------

**required**, **type**: usize

The total number of keyless requests received on the connection.

req_pass
--------

**required**, **type**: usize

The number of requests forwarded successfully.

req_fail
--------

**required**, **type**: usize

The number of requests that failed.

rsp_drop
--------

**required**, **type**: usize

The number of responses dropped before delivery.

rsp_pass
--------

**required**, **type**: usize

The number of responses delivered successfully.

rsp_fail
--------

**required**, **type**: usize

The number of responses marked as failed.
