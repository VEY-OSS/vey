.. _configuration_escaper_trick_float:

trick_float
===========

This escaper picks one of several float escapers at random.

Despite the name, the config format does not support weights. Every configured
candidate has equal selection chance.

No common keys are supported.

next
----

**required**, **type**: seq of :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the candidate next escapers. Each element must be the name of a target float escaper.

.. note:: Duplicate next escapers are ignored.

Example
-------

.. code-block:: yaml

   next:
     - float-a
     - float-b
     - float-c
