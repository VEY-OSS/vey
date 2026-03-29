.. _configuration_escaper_proxy_float:

***********
proxy_float
***********

This escaper reaches the target through remote proxies that are published at
runtime.

The following remote proxy protocols are supported:

* HTTP proxy
* HTTPS proxy
* SOCKS5 proxy

The following interfaces are supported:

* tcp connect
* udp relay (SOCKS5 peers only)
* udp connect (SOCKS5 peers only)
* http(s) forward

This escaper supports the Cap'n Proto RPC ``publish`` command. The published data must be either a single
:ref:`peer <config_escaper_dynamic_peer>` or an array of peers.

Published peers whose ``expire`` time is already in the past are ignored.

The following egress path selection values are supported:

* :ref:`string id <proto_egress_path_selection_string_id>`

  If matched, the :ref:`peer <config_escaper_dynamic_peer>` with the same
  ``id`` is used.

  .. versionadded:: 1.9.2

* :ref:`json value <proto_egress_path_selection_json_value>`

  If matched, the JSON map value is parsed as a
  :ref:`peer <config_escaper_dynamic_peer>` and used directly.

  .. versionadded:: 1.9.2

Config Keys
===========

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`bind_interface <conf_escaper_common_bind_interface>`
* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`tcp_misc_opts <conf_escaper_common_tcp_misc_opts>`
* :ref:`peer negotiation timeout <conf_escaper_common_peer_negotiation_timeout>`
* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

source
------

**optional**, **type**: :external+values:ref:`url str <conf_value_url_str>` | map | null

Set the source used to fetch peers.

Multiple source types are supported. The type is detected from the URL scheme or from the ``type`` key in the map.
See :ref:`sources <config_escaper_dynamic_source>` for the supported formats.

If the selected source maintains local state, a cache file is strongly
recommended so the escaper can continue serving requests before the next
refresh completes.

**default**: passive

cache
-----

**recommend**, **type**: :external+values:ref:`file path <conf_value_file_path>`

Set the cache file.

This is recommended because peer discovery at startup may complete only after the first requests arrive.

The file is created if it does not exist.

The cache path is resolved relative to the config file location.

**default**: not set

refresh_interval
----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set how often peers are refreshed from the configured source.

**default**: 1s

bind_ipv4
---------

**optional**, **type**: :external+values:ref:`ipv4 addr str <conf_value_ipv4_addr_str>`

Set the bind IP address for IPv4 sockets.

**default**: not set

bind_ipv6
---------

**optional**, **type**: :external+values:ref:`ipv6 addr str <conf_value_ipv6_addr_str>`

Set the bind IP address for IPv6 sockets.

**default**: not set

tls_client
----------

**optional**, **type**: bool | :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Enable HTTPS peers and set TLS parameters for the local TLS client.
If set to ``true`` or an empty map, the default TLS client configuration is used.

**default**: not set

tcp_connect_timeout
-------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the application-level TCP connect timeout.

**default**: 30s

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Configure TCP keepalive.

User-level TCP keepalive settings are not applied.

**default**: 60s

expire_guard_duration
---------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

If a peer has an expiration time, it is skipped when adding this guard duration would carry the connection past expiry.

**default**: 5s

.. _config_escaper_dynamic_source:

Sources
=======

For the *map* format, the ``type`` key is always required.

passive
-------

Do not fetch peers. Only RPC publish is used.

The root value of ``source`` may be ``null`` to use the passive source.

Example:

.. code-block:: yaml

   source: null

redis
-----

Fetch peers from a Redis database.

The keys used in the *map* format are:

* sets_key

  **required**, **type**: str

  Set the key of the set that stores peers. Each string entry in the set represents one peer.
  See :ref:`peers <config_escaper_dynamic_peer>` for the supported formats.

* :external+values:ref:`nested redis config map <conf_value_db_redis>`

For *url* str values, the format is:

    redis://[username][:<password>@]<addr>/<db>?sets_key=<sets_key>

Examples:

.. code-block:: yaml

   source:
     type: redis
     addr: 127.0.0.1:6379
     db: 3
     sets_key: proxy_float:peers

.. code-block:: yaml

   source: redis://127.0.0.1:6379/3?sets_key=proxy_float:peers

.. _config_escaper_dynamic_peer:

Peers
=====

Peers are represented as JSON strings whose root element is a map.

At runtime, a request fails if path selection references a peer ID that does
not exist in the current peer set, or if the selected peer has already expired.

Common keys
-----------

* type

  **required**, **type**: str

  Peer type.

.. _config_escaper_dynamic_peer_id:

* id

  **optional**, **type**: str

  Identifier for this peer.

  .. versionadded:: 1.7.23

* addr

  **required**, **type**: :external+values:ref:`sockaddr str <conf_value_sockaddr_str>`

  Set the socket address used to connect to the peer.
  Domain names are not allowed here.

* isp

  **optional**, **type**: str

  ISP for the egress IP address.

* eip

  **optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

  Egress IP address as seen externally.

* area

  **optional**, **type**: :external+values:ref:`egress area <conf_value_egress_area>`

  Area associated with the egress IP address.

* expire

  **optional**, **type**: :external+values:ref:`rfc3339 datetime str <conf_value_rfc3339_datetime_str>`

  Expiration time for this peer.

* tcp_sock_speed_limit

  **optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

  Set the speed limit for each TCP connection to this peer.

Example peer
------------

.. code-block:: json

   {
     "type": "https",
     "id": "corp-edge-1",
     "addr": "203.0.113.30:3128",
     "tls_name": "proxy.example.net",
     "username": "u1",
     "password": "p1",
     "expire": "2026-03-28T08:00:00Z"
   }

Example publish payload
-----------------------

.. code-block:: json

   [
     {
       "type": "http",
       "id": "edge-a",
       "addr": "203.0.113.20:3128"
     },
     {
       "type": "socks5",
       "id": "edge-b",
       "addr": "203.0.113.21:1080",
       "udp_sock_speed_limit": {
         "shift_millis": 1000,
         "max_north": "8MiB",
         "max_south": "8MiB"
       }
     }
   ]

The following types are supported:

http
----

* username

  **optional**, **type**: :external+values:ref:`username <conf_value_username>`

  Username for HTTP Basic authentication.

* password

  **optional**, **type**: :external+values:ref:`password <conf_value_password>`

  Password for HTTP Basic authentication.

* http_connect_rsp_header_max_size

  **optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

  Set the maximum header size accepted for CONNECT responses.

  **default**: 4KiB

* extra_append_headers

  **optional**, **type**: map

  Set extra headers appended to requests sent upstream.
  Keys are header names. Both keys and values must be ASCII strings.

  .. note:: Duplicate headers are not checked. Use this carefully.


https
-----

HTTPS peers support all HTTP peer keys plus the following:

* tls_name

  **optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

  Set the TLS server name used for certificate verification.

  **default**: not set

socks5
------

* username

  **optional**, **type**: :external+values:ref:`username <conf_value_username>`

  Username for SOCKS5 username/password authentication.

* password

  **optional**, **type**: :external+values:ref:`password <conf_value_password>`

  Password for SOCKS5 username/password authentication.

* udp_sock_speed_limit

  **optional**, **type**: :external+values:ref:`udp socket speed limit <conf_value_udp_sock_speed_limit>`

  Set the speed limit for each UDP socket.

  **default**: no limit

  .. versionadded:: 1.7.22

* transmute_udp_peer_ip

  **optional**, **type**: bool or map

  Rewrite the UDP peer IP returned by the remote proxy when needed.

  For a map value, each key is the returned IP and each value is the real IP to use.
  If the map is empty, the peer IP from the TCP connection is used.

  For a boolean value, ``true`` behaves like an empty map and ``false`` disables this feature.

  **default**: false

  .. versionadded:: 1.7.22

* end_on_control_closed

  **optional**, **type**: bool

  End the UDP ASSOCIATE session whenever the peer closes the control TCP connection.

  By default the session will be ended if:

  - Any error occurs on the TCP control connection
  - The TCP control connection closes cleanly after at least one UDP packet has been received

  **default**: false

  .. versionadded:: 1.9.9

socks5s
-------

SOCKS5-over-TLS peers support all SOCKS5 peer keys plus the following:

* tls_name

  **optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

  Set the TLS server name used for certificate verification.

  **default**: not set

.. versionadded:: 1.9.9
