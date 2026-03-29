.. _configuration_auth_user_group_basic:

Basic
=====

This group authenticates users by username and password against the configured
:ref:`token <conf_auth_user_token>` on each user record.

The incoming clear-text password is checked against the stored token, then the
matching user record supplies the rest of the policy for the request.

The following keys are supported:

* :ref:`name <conf_auth_user_group_name>`
* :ref:`type <conf_auth_user_group_type>`
* :ref:`static users <conf_auth_user_group_static_users>`
* :ref:`source <conf_auth_user_group_source>`
* :ref:`cache <conf_auth_user_group_cache>`
* :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
* :ref:`anonymous_user <conf_auth_user_group_anonymous_user>`

Example
-------

.. code-block:: yaml

   name: default
   type: basic
   static_users:
     - name: alice
       token:
         type: xcrypt
         value: '$6$...'
   cache: users-cache.json
   refresh_interval: 60s
