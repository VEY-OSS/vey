.. _configuration:

#############
Configuration
#############

``vey-statsd`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
Its top-level keys are listed below.

At a high level, ``vey-statsd`` is organized as a metrics-processing pipeline:

* ``importer`` receives metrics from upstream senders
* ``collector`` aggregates or rewrites incoming metrics
* ``exporter`` emits the resulting metrics to downstream systems

Process-level settings such as ``runtime`` and ``worker`` define how the daemon
runs, while the pipeline objects define how metrics flow through the service.

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
   * - controller
     - Seq
     - no
     - Controller configuration
   * - importer
     - Mix [#m]_
     - yes
     - Importer configuration, see :doc:`importer/index`
   * - collector
     - Mix [#m]_
     - yes
     - Collector configuration, see :doc:`collector/index`
   * - exporter
     - Mix [#m]_
     - yes
     - Exporter configuration, see :doc:`exporter/index`

Read the pipeline sections listed in the hidden toctree below when you need the
exact keys supported by each importer, collector, or exporter type.

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

.. toctree::
   :hidden:

   runtime
   importer/index
   collector/index
   exporter/index
   values
