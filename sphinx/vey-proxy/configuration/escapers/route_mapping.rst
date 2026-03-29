.. _configuration_escaper_route_mapping:

route_mapping
=============

This escaper maps a user-supplied numeric selector to one of several next
escapers.

The following egress path selection value is supported:

* :ref:`number id <proto_egress_path_selection_number_id>`

  The selected node ID is used to choose the next escaper from ``next``.

  If no number ID is available, a random next escaper is chosen.

No common keys are supported.

next
----

**required**, **type**: seq

Set the candidate next escapers. Each element must be the name of a target float escaper.

.. note:: Duplicate next escapers are not allowed.

Example:

.. code-block:: yaml

   - name: map-egress
     type: route_mapping
     next:
       - float-a
       - float-b
       - float-c
