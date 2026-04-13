.. _configuration:

#############
Configuration
#############

``vey-gateway`` uses YAML for configuration.

The main configuration file is passed with the ``-c`` command-line option.
If a directory is set, the main configuration file is loaded automatically from
it based on the executable binary name.
Its top-level keys are listed below.

At a high level, ``vey-gateway`` is organized around three reusable object
families:

* ``server`` accepts client traffic and defines frontend behavior
* ``discover`` resolves or expands upstream targets
* ``backend`` connects to upstream services and forwards requests

Static daemon settings such as ``runtime``, ``worker``, ``log``, and ``stat``
live alongside those object definitions in the main configuration file.

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
   * - discover
     - Mix [#m]_
     - yes
     - Service-discovery configuration, see :doc:`discovers/index`
   * - backend
     - Mix [#m]_
     - yes
     - Backend configuration, see :doc:`backends/index`
   * - server
     - Mix [#m]_
     - yes
     - Server configuration, see :doc:`servers/index`

Read the object-family sections listed in the hidden toctree below when you
need the detailed keys and behavior for each server, discover, or backend
type.

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

.. toctree::
   :hidden:

   runtime
   log/index
   stat
   discovers/index
   backends/index
   servers/index
   values
