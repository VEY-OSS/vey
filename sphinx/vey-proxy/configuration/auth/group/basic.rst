.. _configuration_auth_user_group_basic:

Basic
=====

This user-group type stores users whose hashed passwords are configured in the
:ref:`token <conf_auth_user_token>` field.

Users are selected by username. The clear-text password is sent to the server,
then hashed and compared with the configured token.

The following keys are supported:

* :ref:`name <conf_auth_user_group_name>`
* :ref:`type <conf_auth_user_group_type>`
* :ref:`static users <conf_auth_user_group_static_users>`
* :ref:`source <conf_auth_user_group_source>`
* :ref:`cache <conf_auth_user_group_cache>`
* :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
* :ref:`anonymous_user <conf_auth_user_group_anonymous_user>`
