.. _configuration_user_group:

****
Auth
****

This section covers how ``vey-proxy`` identifies users, loads user records, and
applies per-user policy.

That includes user groups, individual user records, dynamic user sources,
per-user audit policy, per-site overrides, and username-parameter parsing.

At the top level of ``vey-proxy`` configuration, the object family is named
``user_group``. The alias ``user`` is also accepted by the loader.

.. toctree::
   :maxdepth: 2

   group/index
   user
   source/index
   audit
   site
   name_params
