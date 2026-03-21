.. _configuration:

#############
Configuration
#############

``vey-keyless`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
Its top-level keys are listed below:

+-------------+----------+-------+------------------------------------------------------------+
|Key          |Type      |Reload |Description                                                 |
+=============+==========+=======+============================================================+
|runtime      |Map       |no     |Runtime configuration, see :doc:`runtime`                   |
+-------------+----------+-------+------------------------------------------------------------+
|worker       |Map [#w]_ |no     |Starts unaided worker runtimes if present                   |
+-------------+----------+-------+------------------------------------------------------------+
|log          |Map       |no     |Logging configuration, see :doc:`log/index`                 |
+-------------+----------+-------+------------------------------------------------------------+
|stat         |Map       |no     |Metrics configuration, see :doc:`stat`                      |
+-------------+----------+-------+------------------------------------------------------------+
|controller   |Seq       |no     |Controller configuration                                    |
+-------------+----------+-------+------------------------------------------------------------+
|pre_register |Map       |no     |Pre-registration configuration                              |
+-------------+----------+-------+------------------------------------------------------------+
|server       |Mix [#m]_ |yes    |Server configuration, see :doc:`server`                     |
+-------------+----------+-------+------------------------------------------------------------+
|store        |Mix [#m]_ |yes    |Private-key store configuration, see :doc:`stores/index`    |
+-------------+----------+-------+------------------------------------------------------------+
|backend      |Mix [#m]_ |yes    |Backend configuration, see :doc:`backend`                   |
+-------------+----------+-------+------------------------------------------------------------+

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

.. toctree::
   :hidden:

   runtime
   log/index
   stat
   server
   stores/index
   backend
   values
