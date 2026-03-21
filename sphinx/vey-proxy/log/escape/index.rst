.. _log_escape:

**********
Escape Log
**********

Escape logs record errors that occur while connecting to, negotiating with, or
sending data to a remote peer.

Shared Keys
===========

The following shared keys are present in all escape logs:

escaper_type
------------

**required**, **type**: enum string

  The escaper type.

escaper_name
------------

**required**, **type**: string

  The escaper name.

escape_type
-----------

**required**, **type**: enum string

  The escape log subtype. The meaning of non-shared keys depends on this value.

task_id
-------

**required**, **type**: uuid in simple string format

The UUID of the task.

The same ``task_id`` also appears in the corresponding task log.

upstream
--------

**required**, **type**: domain:port | socket address string

The target upstream the client requested.

next_bound_addr
---------------

**optional**, **type**: socket address string

The local address used for the remote connection.

Present only after a connection to the remote peer has been established.

next_peer_addr
--------------

**optional**, **type**: socket address string

The peer address for the remote connection.

Depending on the escaper type, this may be either the final upstream or the
next proxy peer.

Present only after the next peer address has been selected.

Sub Types
=========

.. toctree::
   :maxdepth: 1

   tcp_connect
   tls_handshake
   udp_sendto
