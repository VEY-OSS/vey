.. _configuration_user_group_source_lua:

Lua
===

Fetches users through a local Lua script.

The following variables are defined when the script is executed:

* __file__

  Absolute path of the script file

  .. versionadded:: 1.11.0

The return value must be the JSON-encoded representation of all dynamic users.

.. note::

  The environment variables `LUA_PATH`_ and `LUA_CPATH`_ can be used to include
  additional Lua modules.
  Any ``;;`` sequence in ``LUA_PATH`` is replaced by the default search path.

  .. _LUA_PATH: https://www.lua.org/manual/5.1/manual.html#pdf-package.path
  .. _LUA_CPATH: https://www.lua.org/manual/5.1/manual.html#pdf-package.cpath

When configured as a map, the following keys are supported:

* fetch_script

  **required**, **type**: :ref:`file path <conf_value_file_path>`

  Path to the Lua script used to fetch dynamic users.

  The content of this script file should be like:

  .. code-block:: lua

    -- TODO fetch users
    local result = "[]"
    -- return the JSON-encoded string
    return result

  **alias**: script

* fetch_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Timeout for running the fetch script.

  It is not recommended to set this value greater than
  :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
  in group config.

  **default**: 30s, **alias**: timeout

* report_script

  **optional**, **type**: :ref:`file path <conf_value_file_path>`

  Path to the Lua script used to report the parsing result for fetched dynamic
  users.

  Two global functions should be defined in this script, like this:

  ..  code-block:: lua

    function reportOk ()
      -- takes no argument
    end

    function reportErr (errMsg)
      -- takes one argument: the error-message string
    end

* report_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Timeout for running the report script.

  It is not recommended to set this value greater than
  :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
  in group config.

  **default**: 15s, **alias**: timeout
