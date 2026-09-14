.. _log_task_websocket:

*********
Websocket
*********

``Websocket`` task logs are emitted by
:ref:`http_guard <configuration_server_http_guard>` for WebSocket tunnels.

HTTP/1 uses ``Upgrade: websocket`` and a ``101 Switching Protocols``
response, then copies bytes until the connection ends. HTTP/2 uses RFC 8441
extended ``CONNECT`` with ``:protocol = websocket``. Both use ``task_type``
``Websocket``. Other ``CONNECT`` or ``Upgrade`` tokens are rejected and are
not this task type.

The tunnel keys follow :ref:`TcpConnect <log_task_tcp_connect>` (including
TCP copy counters, ``Periodic``, ``ClientShutdown``, and
``UpstreamShutdown``). The handshake also records ``version``, ``uri``,
``rsp_status``, and ``origin_status``.

HttpForward-only keys are not present: ``pipeline_wait``, ``user_agent``,
``reuse_connection``, hop durations, or decoded body sizes.

These logs emit:

* ``Created``, when :ref:`flush_task_log_on_created <conf_server_common_flush_task_log_on_created>`
  is enabled
* ``Connected``, when :ref:`flush_task_log_on_connected <conf_server_common_flush_task_log_on_connected>`
  is enabled
* ``Periodic``, when :ref:`task_log_flush_interval <conf_server_common_task_log_flush_interval>`
  is set
* ``ClientShutdown`` / ``UpstreamShutdown``, when one side of the tunnel
  closes first
* ``Finished``, when the task ends

Shared task-log keys from :ref:`log_task` still apply.

.. versionadded:: 1.15.0

version
-------

**required**, **type**: http version string

The HTTP version of the client handshake (``HTTP/1.1`` for HTTP/1 Upgrade,
``HTTP/2.0`` for HTTP/2 extended ``CONNECT``).

uri
---

**required**, **type**: http uri string

The URI from the client handshake. All non-printable characters are escaped.

The max allowed number of characters of the uri is configurable at
:ref:`server <config_server_http_guard_log_uri_max_chars>` or tenant-user
:ref:`log_uri_max_chars <config_user_log_uri_max_chars>` level.

rsp_status
----------

**optional**, **type**: int

The status code in the handshake response sent to the client.

Present after a response has been sent.

origin_status
-------------

**optional**, **type**: int

The status code in the handshake response received from the origin.

Present after the origin handshake response has been received.
