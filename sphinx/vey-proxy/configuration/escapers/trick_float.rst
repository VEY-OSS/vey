.. _configuration_escaper_trick_float:

trick_float
===========

This escaper selects the next float escaper by weighted random choice.

No common keys are supported.

next
----

**required**, **type**: :ref:`metric node name <conf_value_metric_node_name>` | seq

Set the candidate next escapers. Each element must be the name of a target float escaper.

.. note:: Duplicate next escapers are ignored.
