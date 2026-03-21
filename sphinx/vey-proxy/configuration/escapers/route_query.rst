.. _configuration_escaper_route_query:

route_query
===========

This escaper selects the next escaper by querying another service over UDP.

There is no path selection support for this escaper.

No common keys are supported.

.. _configuration_escaper_route_query_fallback_node:

fallback_node
-------------

**required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

Set the fallback escaper name.

query_allowed_next
------------------

**required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

Set the next escapers that are allowed to appear in query results. Each element is a next escaper name.
If the selected escaper is not in this list, the fallback escaper is used.

.. _configuration_escaper_route_query_pass_client_ip:

query_pass_client_ip
--------------------

**optional**, **type**: bool

Set whether ``client_ip`` should also be sent in the query message.

**default**: false

cache_request_batch_count
-------------------------

**optional**, **type**: usize

Set how many consecutive query requests the cache runtime should handle before yielding to the next loop.

**default**: 10

cache_request_timeout
---------------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Set how long to wait for a response from the cache runtime after sending a query request.

The fallback node is used if this times out.

**default**: 100ms

cache_pick_policy
-----------------

**optional**, **type**: :ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select a next proxy address from the query result.

The key for ketama/rendezvous/jump hash is *<client-ip>*.

**default**: ketama

query_peer_addr
---------------

**optional**, **type**: :ref:`env sockaddr str <conf_value_env_sockaddr_str>`

Set the socket address of the service that receives queries.

**default**: 127.0.0.1:1053

query_socket_buffer
-------------------

**optional**, **type**: :ref:`socket buffer config <conf_value_socket_buffer_config>`

Set the socket buffer configuration for the UDP socket used for queries.

**default**: not set

query_wait_timeout
------------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Set how long to wait for a response from the peer service.

If this times out, an empty reply is sent back to the cache runtime.

**default**: 10s

.. _configuration_escaper_route_query_protective_cache_ttl:

protective_cache_ttl
--------------------

**optional**, **type**: usize

Set the cache TTL for failed query results or results with a zero TTL.

**default**: 10

maximum_cache_ttl
-----------------

**optional**, **type**: usize

Set the maximum cache TTL for query results.

**default**: 1800

.. _configuration_escaper_route_query_cache_vanish_wait:

cache_vanish_wait
-----------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Remove a record from the cache after it has remained expired for this long.

Expired records are kept for a short additional period because a fresh query costs more and often returns the same result.

**default**: 30s, **alias**: vanish_after_expire
