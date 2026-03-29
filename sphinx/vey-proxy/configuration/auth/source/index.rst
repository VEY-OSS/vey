.. _configuration_auth_user_source:

******
Source
******

This section describes the supported backends for loading dynamic users.

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

These notes describe the common shape shared by the source types below.

For the :ref:`source <conf_auth_user_group_source>` field on a user group, a
plain URL string is also accepted. In that form, the source type is inferred
from the URL scheme.

.. _conf_auth_user_source_type:

type
----

**required**, **type**: str

Source type.
