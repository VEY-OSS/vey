.. _configuration_escaper_route_select:

route_select
============

This escaper selects the next escaper by applying the configured pick policy.

The following egress path selection value is supported:

* :ref:`string id <proto_egress_path_selection_string_id>`

  If matched, the escaper in :ref:`next_nodes <conf_escaper_route_select_next_nodes>` whose name matches ``ID`` is used.

  The escaper named ``ID`` must be present in :ref:`next_nodes <conf_escaper_route_select_next_nodes>`.
  You can set its weight to ``0`` to prevent it from being selected by default.

  .. versionadded:: 1.7.22

No common keys are supported.

.. _conf_escaper_route_select_next_nodes:

next_nodes
----------

**required**, **type**: :external+values:ref:`weighted metric node name <conf_value_weighted_metric_node_name>` | seq

Set the next escaper or escapers that may be selected.

.. _conf_escaper_route_select_next_pick_policy:

next_pick_policy
----------------

**optional**, **type**: :external+values:ref:`selective pick policy <conf_value_selective_pick_policy>`

Set the policy used to select the next escaper.

The key for ketama/rendezvous/jump hash is *<client-ip>[-<username>]-<upstream-host>*.

**default**: ketama
