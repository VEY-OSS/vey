.. _configuration_escaper_stack_https:

stack_https
===========

.. versionadded:: 1.15.3

This escaper reaches the target through an upstream HTTPS proxy. The TCP
connection to that proxy is opened by the ``next`` escaper. This escaper then
performs a TLS handshake and speaks HTTP proxy on the returned stream.

DNS resolution, source binding, and TCP socket options for that connection
belong to ``next``. This escaper has no ``resolver`` key.

The following interfaces are supported:

* tcp connect
* http(s) forward

The following egress path selection value is supported:

* :ref:`egress upstream <proto_egress_path_selection_egress_upstream>`

  If matched, the ``addr`` field overrides ``proxy_addr`` for this request.

The following common keys are supported:

* :ref:`shared_logger <conf_escaper_common_shared_logger>`
* :ref:`tcp_sock_speed_limit <conf_escaper_common_tcp_sock_speed_limit>`
* :ref:`pass_proxy_userid <conf_escaper_common_pass_proxy_userid>`
* :ref:`peer negotiation timeout <conf_escaper_common_peer_negotiation_timeout>`
* :ref:`peer_health_check <conf_escaper_common_peer_health_check>`
* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`

next
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the escaper that opens the TCP connection to the selected proxy address.

proxy_addr
----------

**required**, **type**: :external+values:ref:`upstream str <conf_value_upstream_str>` | seq

Set the upstream HTTPS proxy address. The default port is ``3128`` and may be
omitted.

If a *seq* is used, each element must be a :external+values:ref:`weighted upstream addr <conf_value_weighted_upstream_addr>`.

A domain name is resolved by ``next``, not by this escaper.

proxy_addr_pick_policy
----------------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select the next proxy address.

The key for ketama/rendezvous/jump hash is *<client-ip>[-<username>]-<upstream-host>*.

**default**: random

tls_client
----------

**optional**, **type**: :external+values:ref:`openssl tls client config <conf_value_openssl_tls_client_config>`

Set TLS parameters for the local TLS client.
If omitted or set to an empty map, the default TLS client configuration is used.

**alias**: ``tls``

tls_name
--------

**optional**, **type**: :external+values:ref:`tls name <conf_value_tls_name>`

Set the TLS server name used to verify peer certificates.

If not set, the host part of the selected proxy address is used.

**default**: not set

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

use_proxy_protocol
------------------

**optional**, **type**: :external+values:ref:`proxy protocol version <conf_value_proxy_protocol_version>`

Set the PROXY protocol version to send on the connection returned by ``next``,
before the TLS handshake.

**default**: not set, which means PROXY protocol won't be used

Example
-------

.. code-block:: yaml

   - name: stacked-https
     type: stack_https
     next: direct
     proxy_addr: proxy.example.net:3128
     tls: {}
     tls_name: proxy.example.net
     proxy_user: service-user
     proxy_passwd: secret
