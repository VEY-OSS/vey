
.. _configure_fs_value_types:

**********
Filesystem
**********

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: available
   - ``vey-statsd``: available

.. _conf_value_hybrid_map:

hybrid map
==========

**yaml value**: seq | str

Hybrid container for a sequence of maps that may be stored in external files.

If the value is a sequence, each element should be either a final map value or
a valid string value as described below.

If the value is a string, it should be a valid path, either absolute or
relative to the directory containing the main configuration file.

The path may be a file or directory:

* If the path is a directory, non-symlink files ending in ``.conf`` are parsed
  as described below.
* If the path is a file, it should contain one or more YAML documents, each of
  which becomes one final map.

.. _conf_value_file_path:

file path
=========

**yaml value**: str

Path to a regular file used in the relevant configuration context.

The path may be absolute or relative to a predefined base path.

Depending on the specific configuration option, the path must either already
exist or be creatable automatically.

.. _conf_value_file:

file
====

**yaml value**: str

Path to a file to be read. It may be absolute or relative to a predefined base
path.

.. _conf_value_absolute_path:

absolute path
=============

**yaml value**: str

Absolute file-system path.

.. _conf_value_directory_path:

directory path
==============

**yaml value**: str

Absolute or context-appropriate path to a directory.

.. availability::

   - ``vey-keyless``: available in ``0.5.0`` and later
   - ``vey-proxy``: not currently used
   - ``vey-gateway``: not currently used
   - ``vey-statsd``: not currently used

.. _conf_value_config_file_format:

config file format
==================

**yaml value**: str

Format of the related configuration file.

The following values are supported:

* yaml
* json

The default varies by context.
