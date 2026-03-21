.. _configuration_server:

******
Server
******

Each server definition is a map with two always-required keys:

* :ref:`name <conf_server_common_name>`, which sets the server name
* :ref:`type <conf_server_common_type>`, which selects the concrete server
  type and therefore the remaining valid keys

The supported server types are documented below.

Servers
=======

.. toctree::
   :maxdepth: 2

   dummy_close
   openssl_proxy
   rustls_proxy
   keyless_proxy
   plain_tcp_port
   plain_quic_port

Common Keys
===========

This section describes keys shared by multiple server types.

.. _conf_server_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the server name.

.. _conf_server_common_type:

type
----

**required**, **type**: str

Set the server type.

.. _conf_server_common_shared_logger:

shared_logger
-------------

**optional**, **type**: ascii

Send this server's task logs to a logger running on a shared thread.

**default**: not set

.. _conf_server_common_listen_in_worker:

listen_in_worker
----------------

**optional**, **type**: bool

Set whether each worker runtime should create its own listening socket when
worker runtimes are enabled.

When enabled, the number of listening instances matches the worker count.

**default**: false

.. _conf_server_common_ingress_network_filter:

ingress_network_filter
----------------------

**optional**, **type**: :external+values:ref:`ingress network acl rule <conf_value_ingress_network_acl_rule>`

Set the client-side network ACL.

The address used for matching is always the interpreted client address. For
servers that listen directly, this is the socket peer address. For servers
behind a PROXY Protocol listener, it is the address carried in the PROXY
Protocol message.

**default**: not set

.. _conf_server_common_tcp_sock_speed_limit:

tcp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`tcp socket speed limit <conf_value_tcp_sock_speed_limit>`

Set the speed limit for each TCP socket.

**default**: no limit

.. _conf_server_common_udp_sock_speed_limit:

udp_sock_speed_limit
--------------------

**optional**, **type**: :external+values:ref:`udp socket speed limit <conf_value_udp_sock_speed_limit>`

Set the speed limit for each UDP socket.

**default**: no limit

.. _conf_server_common_tcp_copy_buffer_size:

tcp_copy_buffer_size
--------------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Set the buffer size used for internal TCP copying.

**default**: 16K, **minimal**: 4K

.. _conf_server_common_tcp_copy_yield_size:

tcp_copy_yield_size
-------------------

**optional**, **type**: :external+values:ref:`humanize usize <conf_value_humanize_usize>`

Set the yield threshold for the internal copy task.

**default**: 1M, **minimal**: 256K

.. _conf_server_common_tcp_misc_opts:

tcp_misc_opts
-------------

**optional**, **type**: :external+values:ref:`tcp misc sock opts <conf_value_tcp_misc_sock_opts>`

Set additional TCP socket options on accepted sockets.

**default**: not set, nodelay is default enabled

.. _conf_server_common_udp_misc_opts:

udp_misc_opts
-------------

**optional**, **type**: :external+values:ref:`udp misc sock opts <conf_value_udp_misc_sock_opts>`

Set additional UDP socket options on created sockets.

**default**: not set

.. _conf_server_common_tls_ticketer:

tls_ticketer
------------

**optional**, **type**: :external+values:ref:`tls ticketer <conf_value_tls_ticketer>`

Set a local or remote rolling TLS ticket key provider.

**default**: not set

.. versionadded:: 0.3.6

task_idle_check_duration
------------------------

**deprecated**

.. versionchanged:: 0.3.8 change default value from 5min to 60s
.. versionchanged:: 0.4.0 deprecated, use `task_idle_check_interval` instead

.. _conf_server_common_task_idle_check_interval:

task_idle_check_interval
------------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Set the idle-check interval for tasks. The effective value is rounded up to a
whole number of seconds.

**default**: 60s, **max**: 30min, **min**: 2s

.. versionadded:: 0.4.0

.. _conf_server_common_task_idle_max_count:

task_idle_max_count
-------------------

**optional**, **type**: usize

Close the task after this many consecutive idle-check results return
``IDLE``.

**default**: 5

.. versionchanged:: 0.3.8 change default value from 1 to 5

.. _conf_server_common_flush_task_log_on_created:

flush_task_log_on_created
-------------------------

**optional**, **type**: bool

Emit a task log when the task is created.

**default**: false

.. versionadded:: 0.3.8

.. _conf_server_common_flush_task_log_on_connected:

flush_task_log_on_connected
---------------------------

**optional**, **type**: bool

Emit a task log when the upstream connection is established.

**default**: false

.. versionadded:: 0.3.8

.. _conf_server_common_task_log_flush_interval:

task_log_flush_interval
-----------------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Enable periodic task logs and set the flush interval.

.. note::

  Periodic task logs are disabled when protocol inspection is enabled, because
  intercept and inspection logs already provide detailed state updates.

**default**: not set

.. versionadded:: 0.3.8

.. _conf_server_common_extra_metrics_tags:

extra_metrics_tags
------------------

**optional**, **type**: :external+values:ref:`static metrics tags <conf_value_static_metrics_tags>`

Set additional metrics tags to attach to server statistics.

**default**: not set
