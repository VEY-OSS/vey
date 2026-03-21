.. _log_task_http_forward:

************
Http Forward
************

All fields documented for :ref:`TcpConnect <log_task_tcp_connect>` task logs
also apply to ``HttpForward`` task logs.

The following keys are specific to ``HttpForward`` task logs:

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
