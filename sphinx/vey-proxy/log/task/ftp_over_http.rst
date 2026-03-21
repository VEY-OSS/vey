.. _log_task_ftp_over_http:

*************
FTP Over HTTP
*************

The following keys are available in ``FTP Over HTTP`` task logs:

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

The target upstream requested by the client.

next_bind_ip
------------

**optional**, **type**: ip address string

The selected bind IP before the connection to the remote peer is attempted.

Present only when bind-IP configuration is enabled on the corresponding
escaper.

next_expire
-----------

**optional**, **type**: rfc3339 timestamp string with microseconds

The expected expiration time of the next peer.

Present only when the next escaper is dynamic and a remote peer has already
been selected.

ftp_c_bound_addr
----------------

**optional**, **type**: socket address string

The local address used for the remote FTP control connection.

Present only after a connection to the remote peer has been established.

ftp_c_peer_addr
---------------

**optional**, **type**: socket address string

The peer address used for the remote FTP control connection.

Depending on the escaper type, this may be either the final upstream or the
next proxy peer.

Present only after the next peer address has been selected.

ftp_c_connect_tries
-------------------

**optional**, **type**: int

Number of attempts made to establish the remote FTP control connection.

ftp_c_connect_spend
-------------------

**optional**, **type**: time duration string

Total time spent establishing the remote FTP control connection, including all
retries.

ftp_d_bound_addr
----------------

**optional**, **type**: socket address string

The local address used for the remote FTP data connection.

Present only after a connection to the remote peer has been established.

ftp_d_peer_addr
---------------

**optional**, **type**: socket address string

The peer address used for the remote FTP data connection.

Depending on the escaper type, this may be either the final upstream or the
next proxy peer.

Present only after the next peer address has been selected.

ftp_d_connect_tries
-------------------

**optional**, **type**: int

Number of attempts made to establish the remote FTP data connection.

ftp_d_connect_spend
-------------------

**optional**, **type**: time duration string

Total time spent establishing the remote FTP data connection, including all
retries.

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

c_rd_bytes
----------

**optional**, **type**: int

Total bytes received from the client.

c_wr_bytes
----------

**optional**, **type**: int

Total bytes sent to the client.

ftp_c_rd_bytes
--------------

**optional**, **type**: int

Total bytes received through the remote FTP control connection.

ftp_c_wr_bytes
--------------

**optional**, **type**: int

Total bytes sent through the remote FTP control connection.

ftp_d_rd_bytes
--------------

**optional**, **type**: int

Total bytes received through the remote FTP data connection.

ftp_d_wr_bytes
--------------

**optional**, **type**: int

Total bytes sent through the remote FTP data connection.
