.. _configuration_escaper_route_mapping:

route_mapping
=============

This escaper selects the next escaper from a user-supplied path selection index.

The following egress path selection value is supported:

* :ref:`number id <proto_egress_path_selection_number_id>`

  The index is used as the index of the next escaper.

  If no index is available from path selection, a random next escaper is chosen.

No common keys are supported.

next
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the candidate next escapers. Each element must be the name of a target float escaper.

.. note:: Duplicate next escapers are not allowed.
