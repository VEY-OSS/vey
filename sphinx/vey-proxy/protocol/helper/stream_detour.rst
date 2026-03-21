.. _protocol_helper_stream_detour:

=============
Stream Detour
=============

An external interception server can implement this to intercept protocols that are configured
with the ``detour`` inspection policy in the :ref:`auditor <configuration_auditor>`
configuration. Each supported protocol has its own configuration option.

The external server should listen on a QUIC port. Configure that endpoint in
the auditor by setting
:ref:`stream detour service <conf_auditor_stream_detour_service>`.

``vey-proxy`` establishes a pool of idle QUIC connections to that port in
advance. When interception is needed for a client-to-remote stream, it opens
two bidirectional QUIC streams: a north stream and a south stream.

North Stream
------------

The north stream carries data flowing from the client to the remote peer.

At the beginning of the stream, ``vey-proxy`` sends a **PROXY Protocol v2
header** and an optional **payload** to the external server.

The PPv2 Type-Values are:

* 0xE0 | Upstream Address

  The target upstream address, encoded in UTF-8 without a trailing ``\0``.
  This will always be set.

* 0xE2 | Username

  The client's username, encoded in UTF-8 without a trailing ``\0``.
  This will be set only if client auth is enabled.

* 0xE3 | Task ID

  The task ID in binary UUID format. This is always set.

* 0xE4 | Protocol

  The detected protocol string, encoded in UTF-8 without a trailing ``\0``.
  This will always be set.

  The supported values are listed in
  :ref:`Protocol and Payload <stream_detour_protocol_payload>`.

* 0xE5 | Match ID

  The ID used to pair the north stream with the south stream.
  The value is a 2-byte ``uint16`` in big-endian order.

* 0xE6 | Payload Length

  The length of the extra payload data. The payload format depends on the
  selected *protocol*.
  The value is a 4-byte ``uint32`` in big-endian order.
  If the length is greater than ``0``, the payload data immediately follows the
  PPv2 header.

  The payload format is described in
  :ref:`Protocol and Payload <stream_detour_protocol_payload>`.

After sending the PPv2 header, ``vey-proxy`` waits for a 4-byte response from
the external server.

- The first 2 bytes should be a ``uint16`` in big-endian order.
- The last 2 bytes should be a ``uint16`` action code in big-endian order. The
  supported actions are:

  * 0 - continue

    Continue forwarding data. The flow becomes
    ``client_read -> detour_server -> remote_write``.

  * 1 - bypass

    Skip the detour server and transfer data directly between client and
    remote.

  * 2 - block

    Block the client-to-remote transfer and close the connection immediately.

South Stream
------------

The south stream carries data flowing from the remote peer back to the client.

At the beginning of the stream, ``vey-proxy`` sends a PROXY Protocol v2 header
to the external server.

The PPv2 Type-Values are:

* 0xE5 | Match ID

  The ID used to pair the north stream with the south stream.
  The value is a 2-byte ``uint16`` in big-endian order.

After that header is sent, the data flow becomes
``remote_read -> detour_server -> client_write``.

.. _stream_detour_protocol_payload:

Protocol and Payload
--------------------

HTTP 2
^^^^^^

**protocol value**: http_2

**payload format**: no payload

WebSocket
^^^^^^^^^

**protocol value**: websocket

**payload format**:

The payload is multi-line text. Each line ends with ``\r\n``.

The first line is the */resource name/*.

The remaining lines are copied from the HTTP headers used during the Upgrade
handshake. Possible headers include:

- Host in request
- Origin in request
- Sec-WebSocket-Key in request
- Sec-WebSocket-Version in request
- Sec-WebSocket-Accept in response
- Sec-WebSocket-Protocol in response
- Sec-WebSocket-Extensions in response

SMTP
^^^^

**protocol value**: smtp

**payload format**: no payload

IMAP
^^^^

**protocol value**: imap

**payload format**: no payload
