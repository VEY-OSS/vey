.. _log_task_h2_forward:

**********
H2 Forward
**********

``H2Forward`` and ``H2Websocket`` task logs are emitted by
:ref:`http_guard <configuration_server_http_guard>` for HTTP/2 streams.
Each client stream is one task. ``H2Websocket`` is the RFC 8441
extended-``CONNECT`` WebSocket case; the keys are the same.

``task_type`` is ``H2Forward`` or ``H2Websocket``.

These logs emit:

* ``Created``, when :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
  is enabled
* ``Finished``, when the stream ends

They do **not** emit ``Connected``, ``Periodic``, ``ClientShutdown``, or
``UpstreamShutdown``. There is no ``reason`` key: the slog record message is
``finished`` on success, or the error string on failure.

Shared task-log keys from :ref:`log_task` still apply
(``server_type``, ``server_name``, ``task_id``, ``stage``, ``start_at``,
``wait_time``, ``ready_time``, ``total_time``). ``user`` is the site tenant
username when the site has an :ref:`owner <conf_site_owner>`.

Compared with :ref:`HttpForward <log_task_http_forward>`, these logs do not
include ``pipeline_wait``, ``user_agent``, ``next_expire``,
``tcp_connect_tries``, ``tcp_connect_spend``, or the TCP copy counters
(``c_rd_bytes``, ``c_wr_bytes``, ``r_rd_bytes``, ``r_wr_bytes``).
Connection-level HTTP/2 I/O is counted on the ``h2_connection`` metrics
request type instead.

.. versionadded:: 1.15.0

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

The site origin this stream was forwarded to.

escaper
-------

**optional**, **type**: string

The selected escaper name. Present on ``Finished`` records.

next_bind_ip
------------

**optional**, **type**: ip address string

The selected bind IP before the origin connection is attempted.

Present only on ``Finished`` records, and only when bind-IP configuration is
enabled on the corresponding escaper.

next_bound_addr
---------------

**optional**, **type**: socket address string

The local address used for the origin connection.

Present only on ``Finished`` records after an origin connection has been
established.

next_peer_addr
--------------

**optional**, **type**: socket address string

The peer address used for the origin connection.

Depending on the escaper type, this may be either the final origin or the
next proxy peer.

Present only on ``Finished`` records after the next peer address has been
selected.

reuse_connection
----------------

**optional**, **type**: bool

Whether this stream reused an existing origin HTTP/2 connection from the
site pool.

Present only on ``Finished`` records.

method
------

**required**, **type**: http method string

The HTTP method from the client request.

uri
---

**required**, **type**: http uri string

The URI from the client request. All non-printable characters are escaped.

The max allowed number of characters of the uri is configurable at
:ref:`server <config_server_http_guard_log_uri_max_chars>` or tenant-user
:ref:`log_uri_max_chars <config_user_log_uri_max_chars>` level.

rsp_status
----------

**optional**, **type**: int

The status code in the response sent to the client.

Present only on ``Finished`` records.

origin_status
-------------

**optional**, **type**: int

The status code in the response received from the origin.

Present only on ``Finished`` records.

dur_req_send_hdr
----------------

**optional**, **type**: time duration string

The time spent between task creation and sending the request header to the
origin.

Present only on ``Finished`` records.

dur_req_send_all
----------------

**optional**, **type**: time duration string

The time spent between task creation and sending the full request to the
origin.

Present only on ``Finished`` records.

dur_rsp_recv_hdr
----------------

**optional**, **type**: time duration string

The time spent between task creation and receiving the response header from
the origin.

Present only on ``Finished`` records.

dur_rsp_recv_all
----------------

**optional**, **type**: time duration string

The time spent between task creation and receiving the full response from the
origin.

Present only on ``Finished`` records.

clt_req_body_size
-----------------

**optional**, **type**: int

Decoded request body size received from the client, in bytes.

This is the HTTP/2 ``DATA`` payload (and, with ICAP REQMOD, the original
client body). It is not the on-wire frame size and is not the same as a TCP
copy counter.

``0`` when the request has no body. If this hop has started, the value is the
decoded payload received so far, including when the task later fails. Omitted
if the client request body has not started.

With ICAP REQMOD, this may differ from ``ups_req_body_size``.

Present only on ``Finished`` records.

ups_req_body_size
-----------------

**optional**, **type**: int

Decoded request body size sent to the origin, in bytes.

See ``clt_req_body_size`` for how the size is measured. ``0`` when no request
body is sent origin-side. If this hop has started, the value is the decoded
payload sent so far, including when the task later fails. Omitted if the
origin request body has not started.

With ICAP REQMOD, this is the adapted body sent origin-side and may differ
from ``clt_req_body_size``.

Present only on ``Finished`` records.

ups_rsp_body_size
-----------------

**optional**, **type**: int

Decoded response body size received from the origin, in bytes.

See ``clt_req_body_size`` for how the size is measured. ``0`` when the origin
response has no body. If this hop has started, the value is the decoded
payload received so far, including when the task later fails. Omitted if the
origin response body has not started.

With ICAP RESPMOD, this is the original origin body and may differ from
``clt_rsp_body_size``.

Present only on ``Finished`` records.

clt_rsp_body_size
-----------------

**optional**, **type**: int

Decoded response body size sent to the client, in bytes.

See ``clt_req_body_size`` for how the size is measured. ``0`` when no response
body is sent to the client. If this hop has started, the value is the decoded
payload sent so far, including when the task later fails. Omitted if the
client response body has not started.

With ICAP RESPMOD, this is the adapted body sent to the client and may differ
from ``ups_rsp_body_size``.

Present only on ``Finished`` records.
