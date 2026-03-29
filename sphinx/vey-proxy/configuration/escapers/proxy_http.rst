.. _configuration_escaper_proxy_http:

proxy_http
==========

This escaper reaches the target through an upstream HTTP proxy.

The following interfaces are supported:

* tcp connect
* http(s) forward

The following egress path selection value is supported:

* :ref:`egress upstream <proto_egress_path_selection_egress_upstream>`

  If matched, the ``addr`` field overrides ``proxy_addr`` for this request.

  .. versionadded:: 1.13.0

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`resolver <conf_escaper_common_resolver>`, **required** only if *proxy_addr* is domain
* :ref:`resolve_strategy <conf_escaper_common_resolve_strategy>`
* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`bind_interface <conf_escaper_common_bind_interface>`
* :ref:`no_ipv4 <conf_escaper_common_no_ipv4>`
* :ref:`no_ipv6 <conf_escaper_common_no_ipv6>`
* :ref:`tcp_connect <conf_escaper_common_tcp_connect>`
* :ref:`happy eyeballs <conf_escaper_common_happy_eyeballs>`
* :ref:`tcp_misc_opts <conf_escaper_common_tcp_misc_opts>`
* :ref:`pass_proxy_userid <conf_escaper_common_pass_proxy_userid>`
* :ref:`peer negotiation timeout <conf_escaper_common_peer_negotiation_timeout>`
* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

proxy_addr
----------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>` | seq

Set the target proxy address. The default port is ``3128`` and may be omitted.

If a *seq* is used, each element must be a :external+values:ref:`weighted upstream addr <conf_value_weighted_upstream_addr>`.

If any configured proxy address uses a domain name, ``resolver`` becomes
required.

proxy_addr_pick_policy
----------------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select the next proxy address.

The key for ketama/rendezvous/jump hash is *<client-ip>[-<username>]-<upstream-host>*.

**default**: random

proxy_username
--------------

**optional**, **type**: :external+values:ref:`username <conf_value_username>`

Set the proxy username. Basic authentication is used by default.

**alias**: ``proxy_user``

.. note::

  This conflicts with :ref:`pass_proxy_userid <conf_escaper_common_pass_proxy_userid>`.

proxy_password
--------------

**optional**, **type**: :external+values:ref:`password <conf_value_password>`

Set the proxy password. Required if username is present.

**alias**: ``proxy_passwd``

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

http_forward_capability
-----------------------

**optional**, **type**: :external+values:ref:`http forward capability <conf_value_http_forward_capability>`

Set the HTTP forwarding capabilities supported by the next proxy.

**default**: all capability disabled

http_connect_rsp_header_max_size
--------------------------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Set the maximum header size accepted for CONNECT responses.

**default**: 4KiB

tcp_keepalive
-------------

**optional**, **type**: :external+values:ref:`tcp keepalive <conf_value_tcp_keepalive>`

Configure TCP keepalive.

User-level TCP keepalive settings are not applied.

**default**: no keepalive set

use_proxy_protocol
------------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

Set the PROXY protocol version to use after the TCP connection to the peer is established.

**default**: not set, which means PROXY protocol won't be used

Example:

.. code-block:: yaml

   - name: corp-http-proxy
     type: proxy_http
     proxy_addr:
       - addr: proxy-a.example.net:3128
         weight: 2
       - addr: proxy-b.example.net:3128
         weight: 1
     resolver: local-dns
     proxy_user: service-user
     proxy_passwd: secret
     proxy_addr_pick_policy: rendezvous
