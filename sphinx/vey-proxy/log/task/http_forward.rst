.. _log_task_http_forward:

************
Http Forward
************

All fields documented for :ref:`TcpConnect <log_task_tcp_connect>` task logs
also apply to ``HttpForward`` task logs.

The following keys are specific to ``HttpForward`` task logs. The four decoded
body-size keys below are also present on ``H2Forward`` task logs and intercept
``HttpForward`` / ``H2StreamForward`` logs.

pipeline_wait
-------------

**required**, **type**: time duration string

The time spent between receiving the HTTP request header and creating the task.

reuse_connection
----------------

**optional**, **type**: bool

Whether this task reused an existing remote connection.

method
------

**required**, **type**: http method string

The HTTP method from the client request.

uri
---

**required**, **type**: http uri string

The URI from the client request. All non-printable characters are escaped.

The max allowed number of characters of the uri is configurable at
:ref:`server <config_server_http_proxy_log_uri_max_chars>` or :ref:`user <config_user_log_uri_max_chars>` level.

user_agent
----------

**optional**, **type**: string

The first ``User-Agent`` header value in the client request.

rsp_status
----------

**optional**, **type**: int

The status code in the response sent to the client.

origin_status
-------------

**optional**, **type**: int

The status code in the response received from the remote peer.

dur_req_send_hdr
----------------

**optional**, **type**: time duration string

The time spent between task creation and sending the request header to the
remote peer.

dur_req_send_all
----------------

**optional**, **type**: time duration string

The time spent between task creation and sending the full request to the remote
peer.

dur_rsp_recv_hdr
----------------

**optional**, **type**: time duration string

The time spent between task creation and receiving the response header from the
remote peer.

dur_rsp_recv_all
----------------

**optional**, **type**: time duration string

The time spent between task creation and receiving the full response from the
remote peer.

clt_req_body_size
-----------------

**optional**, **type**: int

Decoded request body size received from the client, in bytes.

This is the HTTP message-body payload after transfer-coding is removed
(HTTP/1.1 chunked payload, HTTP/2 ``DATA`` payload). It is not the on-wire
byte count and is not the same as ``c_rd_bytes``.

``0`` when the request has no body. Omitted while the client body has not been
fully read (for example the upstream responded before the request body
finished).

With ICAP REQMOD, this is the original client body and may differ from
``ups_req_body_size``.

.. versionadded:: 1.15.0

ups_req_body_size
-----------------

**optional**, **type**: int

Decoded request body size sent to the upstream, in bytes.

See ``clt_req_body_size`` for how the size is measured. ``0`` when no request
body is sent upstream. Omitted while the upstream request body has not been
fully sent.

With ICAP REQMOD, this is the adapted body sent upstream and may differ from
``clt_req_body_size``.

.. versionadded:: 1.15.0

ups_rsp_body_size
-----------------

**optional**, **type**: int

Decoded response body size received from the upstream, in bytes.

See ``clt_req_body_size`` for how the size is measured. ``0`` when the upstream
response has no body. Omitted while the upstream response body has not been
fully received.

With ICAP RESPMOD, this is the original upstream body and may differ from
``clt_rsp_body_size``.

.. versionadded:: 1.15.0

clt_rsp_body_size
-----------------

**optional**, **type**: int

Decoded response body size sent to the client, in bytes.

See ``clt_req_body_size`` for how the size is measured. ``0`` when no response
body is sent to the client. Omitted while the client response body has not been
fully sent.

With ICAP RESPMOD, this is the adapted body sent to the client and may differ
from ``ups_rsp_body_size``.

.. versionadded:: 1.15.0

