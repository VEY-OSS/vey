.. _configuration:

#############
Configuration
#############

``vey-proxy`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
If a directory is set, the main configuration file is loaded automatically from
it based on the executable binary name.
Its top-level keys are listed below.

Multiple YAML documents in the same file are accepted and are merged as if they
were one configuration file.

At a high level, ``vey-proxy`` is organized around a small set of reusable
object families:

* ``server`` accepts inbound traffic and defines frontend behavior
* ``escaper`` controls how outbound traffic leaves the daemon
* ``resolver`` defines DNS resolution behavior
* ``user_group`` defines authentication, identity, and policy
* ``auditor`` adds inspection, interception, or adaptation logic

Static process-level settings such as ``runtime``, ``worker``, ``log``, and
``stat`` live at the top level of the main configuration file alongside those
reusable objects.

.. list-table::
   :header-rows: 1

   * - Key
     - Type
     - Reload
     - Description
   * - runtime
     - Map
     - no
     - Runtime configuration, see :doc:`runtime`
   * - worker
     - Map [#w]_
     - no
     - Starts unaided worker runtimes if present
   * - log
     - Map
     - no
     - Logging configuration, see :doc:`log/index`
   * - stat
     - Map
     - no
     - Metrics configuration, see :doc:`stat`
   * - controller
     - Seq
     - no
     - Controller configuration
   * - resolver
     - Mix [#m]_
     - yes
     - Resolver configuration, see :doc:`resolvers/index`
   * - escaper
     - Mix [#m]_
     - yes
     - Escaper configuration, see :doc:`escapers/index`
   * - user_group
     - Mix [#m]_
     - yes
     - User-group configuration, see :doc:`auth/index`
   * - auditor
     - Mix [#m]_
     - yes
     - Auditor configuration, see :doc:`auditors/index`
   * - server
     - Mix [#m]_
     - yes
     - Server configuration, see :doc:`servers/index`

Read the object-family sections listed in the hidden toctree below when you
need the detailed keys and behavior for each component type.

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

The top-level key ``user`` is accepted as an alias of ``user_group``.

.. toctree::
   :hidden:

   runtime
   log/index
   stat
   resolvers/index
   escapers/index
   auditors/index
   auth/index
   servers/index
   values
