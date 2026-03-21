.. _configuration_user_group_source_file:

File
====

Fetches dynamic users from a local file.

The file content should be the JSON-encoded representation of all dynamic
users.

When configured as a map, the following keys are supported:

* path

  **required**, **type**: :external+values:ref:`file path <conf_value_file_path>`

  Path to the file. The file must exist before the daemon starts.

* format

  **optional**, **type**: :external+values:ref:`config file format <conf_value_config_file_format>`

  Format of the file specified in ``path``.

  **default**: If the file has an extension, the extension is used to detect
  the format. If the format cannot be inferred from the extension, ``yaml`` is
  used.

For URL-style string values, the path must be absolute and use the following
format:

    file://<path>[?[format=<format>]]

.. note:: Published users are not cached when a static file source is used.
