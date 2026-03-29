.. _configuration_user_group_source_python:

Python
======

Loads dynamic users by calling a local Python script.

When Python is used as the source, the on-disk group cache is written in JSON
format.

The following variables are defined when the script is executed:

* __file__

  Absolute path of the script file

  .. versionadded:: 1.11.0

When configured as a map, the following keys are supported:

* script

  **required**, **type**: :external+values:ref:`file path <conf_value_file_path>`

  Path to the Python script.

  Three global functions should be defined in this script, like this:

  ..  code-block:: python

    def fetch_users():
        # required, takes no argument, returns the JSON string
        return "[]"

    def report_ok():
        # optional, takes no argument
        pass

    def report_err(errmsg):
        # optional, takes one positional argument, which is the error message string
        pass

* fetch_timeout

  **optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

  Timeout for running the fetch function.

  It is not recommended to set this value greater than
  :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
  in group config.

  **default**: 30s

* report_timeout

  **optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

  Timeout for running the report functions.

  It is not recommended to set this value greater than
  :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
  in group config.

  **default**: 15s

Example
-------

.. code-block:: yaml

   source:
     type: python
     script: fetch_users.py
     fetch_timeout: 10s
     report_timeout: 5s
