.. _configuration:

#############
Configuration
#############

``vey-proxy`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
Its top-level keys are listed below:

+-----------+----------+-------+-----------------------------------------------------+
|Key        |Type      |Reload |Description                                          |
+===========+==========+=======+=====================================================+
|runtime    |Map       |no     |Runtime configuration, see :doc:`runtime`            |
+-----------+----------+-------+-----------------------------------------------------+
|worker     |Map [#w]_ |no     |Starts unaided worker runtimes if present            |
+-----------+----------+-------+-----------------------------------------------------+
|log        |Map       |no     |Logging configuration, see :doc:`log/index`          |
+-----------+----------+-------+-----------------------------------------------------+
|stat       |Map       |no     |Metrics configuration, see :doc:`stat`               |
+-----------+----------+-------+-----------------------------------------------------+
|controller |Seq       |no     |Controller configuration                             |
+-----------+----------+-------+-----------------------------------------------------+
|resolver   |Mix [#m]_ |yes    |Resolver configuration, see :doc:`resolvers/index`   |
+-----------+----------+-------+-----------------------------------------------------+
|escaper    |Mix [#m]_ |yes    |Escaper configuration, see :doc:`escapers/index`     |
+-----------+----------+-------+-----------------------------------------------------+
|user_group |Mix [#m]_ |yes    |User-group configuration, see :doc:`auth/index`      |
+-----------+----------+-------+-----------------------------------------------------+
|auditor    |Mix [#m]_ |yes    |Auditor configuration, see :doc:`auditors/index`     |
+-----------+----------+-------+-----------------------------------------------------+
|server     |Mix [#m]_ |yes    |Server configuration, see :doc:`servers/index`       |
+-----------+----------+-------+-----------------------------------------------------+

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

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
