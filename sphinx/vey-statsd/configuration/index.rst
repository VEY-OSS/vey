.. _configuration:

#############
Configuration
#############

``vey-statsd`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
Its top-level keys are listed below:

+-------------+----------+-------+------------------------------------------------------+
|Key          |Type      |Reload |Description                                           |
+=============+==========+=======+======================================================+
|runtime      |Map       |no     |Runtime configuration, see :doc:`runtime`             |
+-------------+----------+-------+------------------------------------------------------+
|worker       |Map [#w]_ |no     |Starts unaided worker runtimes if present             |
+-------------+----------+-------+------------------------------------------------------+
|controller   |Seq       |no     |Controller configuration                              |
+-------------+----------+-------+------------------------------------------------------+
|importer     |Mix [#m]_ |yes    |Importer configuration, see :doc:`importer/index`     |
+-------------+----------+-------+------------------------------------------------------+
|collector    |Mix [#m]_ |yes    |Collector configuration, see :doc:`collector/index`   |
+-------------+----------+-------+------------------------------------------------------+
|exporter     |Mix [#m]_ |yes    |Exporter configuration, see :doc:`exporter/index`     |
+-------------+----------+-------+------------------------------------------------------+

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
