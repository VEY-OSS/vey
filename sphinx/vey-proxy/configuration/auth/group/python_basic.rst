.. _configuration_auth_user_group_python_basic:

Python Basic
============

.. attention::

   This requires the ``python`` build feature, which is enabled by default.

This group authenticates users by invoking a Python script that provides a
``check_password(username, password)`` function. It returns ``True`` when the
credentials are valid and ``False`` otherwise.

Static and dynamic users are still supported. The Python script verifies the
presented password, then ``vey-proxy`` loads the rest of the user policy from
the static list, the dynamic source, or ``unmanaged_user``.

A thread-local LRU cache is used to avoid calling the script for every request.

The following common keys are supported:

* :ref:`name <conf_auth_user_group_name>`
* :ref:`type <conf_auth_user_group_type>`
* :ref:`static users <conf_auth_user_group_static_users>`
* :ref:`source <conf_auth_user_group_source>`
* :ref:`cache <conf_auth_user_group_cache>`
* :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
* :ref:`anonymous_user <conf_auth_user_group_anonymous_user>`

script
------

**required**, **type**: :external+values:ref:`file path <conf_value_file_path>`

Path to the Python script file.

The script must define a ``check_password(username, password)`` function that
accepts two string arguments and returns a boolean.

The script is re-read from disk on each authentication attempt, so changes take
effect without reloading the user group.

unmanaged_user
--------------

**optional**, **type**: :ref:`user <configuration_auth_user>`

Configures and enables unmanaged users.

This is a template user configuration for users who authenticate successfully
with the Python script but are not defined in either the static or dynamic user
lists.

If not set, only static or dynamic users will be allowed.

**default**: not set

check_timeout
-------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Timeout for each Python ``check_password()`` call.

**default**: 4s

cache_user_count
----------------

**optional**, **type**: non-zero usize

Maximum number of users stored in the thread-local LRU cache.

**default**: 128

cache_expire_time
-----------------

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Expiration time for valid passwords in the thread-local LRU cache.

**default**: 5min

Example
-------

.. code-block:: yaml

   name: script-auth
   type: python_basic
   script: /etc/vey-proxy/auth_check.py
   cache_user_count: 256
   cache_expire_time: 10min
   unmanaged_user:
     name: python-template
     explicit_sites: []

Example Python script:

.. code-block:: python

   def check_password(username, password):
       # Implement your authentication logic here
       # Return True if the credentials are valid, False otherwise
       return username == "admin" and password == "secret"
