.. _configuration_auth_user_group:

**********
User Group
**********

Users are divided into two categories: static and dynamic.
Static users are configured through
:ref:`static users <conf_auth_user_group_static_users>` in the YAML
configuration.
Dynamic users are fetched periodically from
:ref:`source <conf_auth_user_group_source>`, usually in JSON format.
Both are optional and share the same underlying data structure.

The Cap'n Proto RPC ``publish_dynamic_users`` command is supported. The
published data should be an array of
:ref:`user <configuration_auth_user>`.

Each user-group configuration item is a map with two required keys:

* :ref:`name <conf_auth_user_group_name>`: user-group name
* :ref:`type <conf_auth_user_group_type>`: authentication type

Groups
======

.. toctree::
   :maxdepth: 1

   basic
   facts
   ldap

Common Keys
===========

.. _conf_auth_user_group_name:

name
----

**required**,  **type**: :ref:`metric node name <conf_value_metric_node_name>`

The name of the user group.

.. _conf_auth_user_group_type:

type
----

**required**, **type**: str

Authentication type of the user group. It also determines how the remaining
keys are interpreted.

.. _conf_auth_user_group_static_users:

**default**: basic

.. versionadded:: 1.13.0

static_users
------------

**optional**, **type**: seq

Static users can be added in this array.

See :ref:`user <configuration_auth_user>` for detailed structure of user.

.. _conf_auth_user_group_source:

source
------

**optional**, **type**: :ref:`url str <conf_value_url_str>` | map

Source used to fetch dynamic users.

Multiple source types are supported. The type is detected either from the
scheme of the URL or from the ``type`` key in the map.
See :ref:`source <configuration_auth_user_source>` for the supported source
types.

Existing dynamic users are updated in place. If fetching fails, the previously
loaded users are kept.

**default**: not set

.. _conf_auth_user_group_cache:

cache
-----

**optional**, **type**: :ref:`file path <conf_value_file_path>`

Local file used to cache remote results. It is read during initial user-group
loading.

The file will be created if not existed.

.. note:: This should be set if you want to publish dynamic users.

**default**: not set

.. versionadded:: 1.7.22

.. _conf_auth_user_group_refresh_interval:

refresh_interval
----------------

**optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

Interval used for user-expiration checks and for refreshing dynamic users.

**default**: 60s

.. _conf_auth_user_group_anonymous_user:

anonymous_user
--------------

**optional**, **type**: :ref:`user <configuration_auth_user>`

Configures and enables the anonymous user.

This user is used when no matching username is found in either the static or
dynamic users, or when the client request carries no authentication
information.

**default**: not set

.. versionadded:: 1.7.13
