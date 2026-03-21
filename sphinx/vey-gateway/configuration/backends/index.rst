.. _configuration_backend:

*******
Backend
*******

Each backend definition is a map with two always-required keys:

* :ref:`name <conf_backend_common_name>`, which sets the backend name
* :ref:`type <conf_backend_common_type>`, which selects the concrete backend
  type and therefore the remaining valid keys

The supported backend types are documented below.

Backends
========

.. toctree::
   :maxdepth: 2

   dummy_close
   keyless_quic
   keyless_tcp
   stream_tcp

Common Keys
===========

This section describes keys shared by multiple backend types.

.. _conf_backend_common_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the backend name.

.. _conf_backend_common_type:

type
----

**required**, **type**: str

Set the backend type.

.. _conf_backend_common_discover:

discover
--------

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

Set the discover instance used by this backend.

.. _conf_backend_common_discover_data:

discover_data
-------------

**required**, **type**: :ref:`discover register data <conf_discover_register_data>`

Set the registration data passed to
:ref:`discover <conf_backend_common_discover>`.

.. _conf_backend_common_extra_metrics_tags:

extra_metrics_tags
------------------

**optional**, **type**: :external+values:ref:`static metrics tags <conf_value_static_metrics_tags>`

Set additional metrics tags to attach to backend statistics.

**default**: not set
