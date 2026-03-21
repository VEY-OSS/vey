.. _configuration_auth_user_source:

******
Source
******

This section defines where dynamic-user configuration can be loaded from.

Each source configuration is a map with one required key:

* :ref:`type <conf_auth_user_source_type>`, which selects the source type and
  therefore determines how the remaining keys are interpreted

Sources
=======

.. toctree::
   :maxdepth: 1

   file
   lua
   python

Common Keys
===========

This section describes common keys shared by multiple source types.

.. _conf_auth_user_source_type:

type
----

**required**, **type**: str

Source type.
