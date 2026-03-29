.. _configuration_auth_user_group_facts:

Facts
=====

This group authenticates users by matching connection facts instead of checking
username and password pairs.

This group reuses the same static-user, dynamic-source, cache, and anonymous
user handling as :ref:`Basic <configuration_auth_user_group_basic>`, but user
matching is driven by each user's ``match_by_facts`` rules instead of username
and password.

The following common keys are supported:

* :ref:`name <conf_auth_user_group_name>`
* :ref:`type <conf_auth_user_group_type>`
* :ref:`static users <conf_auth_user_group_static_users>`
* :ref:`source <conf_auth_user_group_source>`
* :ref:`cache <conf_auth_user_group_cache>`
* :ref:`refresh_interval <conf_auth_user_group_refresh_interval>`
* :ref:`anonymous_user <conf_auth_user_group_anonymous_user>`
