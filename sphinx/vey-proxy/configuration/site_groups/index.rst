.. _configuration_site_group:

**********
Site Group
**********

A site group is a named Host / SNI lookup table. Reverse-proxy servers such as
:ref:`http_expose <configuration_server_http_rproxy>` match the request host
against the group, then take the site's upstream and TLS settings.

This is not the same object as a :ref:`user site <configuration_auth_user_site>`.
User sites live on a forward-proxy user (``explicit_sites``) and apply after
the client is authenticated. A site group is selected first by Host / SNI and
is used to pick the origin.

The top-level configuration key is ``site_group``. Each item is a map. The
value may be an inline sequence or a directory of files; see
:external+values:ref:`hybrid map <conf_value_hybrid_map>`.

If a server names a missing site group, ``vey-proxy`` falls back to an empty
group. Requests then have no matching site.

Reload a single group with ``vey-proxy-ctl reload-site-group <name>``. Sites
keep their stats and limiters when the site ID is unchanged. Every online
``http_expose`` server that references the group then rebuilds its host table.
``vey-proxy-ctl list site-group`` lists loaded group names.

.. versionadded:: 1.15.0

.. toctree::
   :maxdepth: 1

   site

Group Keys
==========

.. _conf_site_group_name:

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

The site-group name. Servers reference it with ``site_group``.

.. _conf_site_group_static_sites:

static_sites
------------

**optional**, **type**: :external+values:ref:`host matched object <conf_value_host_matched_object>` <:ref:`site <configuration_site>`>, **alias**: sites

The sites in this group. Match keys (``exact_match``, ``suffix_match`` /
``child_match``, ``set_default``) select the site. All other keys belong to
the :ref:`site <configuration_site>`.

A site may list several host rules. Those rules share one upstream and one
pair of TLS settings. The same ``exact_match`` or ``suffix_match`` value
cannot appear on two sites in the same group.

Lookup uses the HTTP ``Host`` header and, when TLS is enabled, the ClientHello
SNI. Matching is exact host, then suffix, then the group default.

A site with no match rule and ``set_default: false`` is unused unless it is
the only site in the value, in which case it becomes the default.

**default**: empty

Example
=======

.. code-block:: yaml

   site_group:
     - name: local
       static_sites:
         - id: app
           exact_match:
             - app.example.net
             - www.app.example.net
           upstream: 127.0.0.1:8080
         - id: example_org
           suffix_match: example.org
           set_default: true
           upstream: 10.1.2.3:8080
           tls_client: {}
           tls_name: origin.example.org
           task_idle_max_count: 3
           tcp_connect:
             max_retry: 2
             each_timeout: 5s

   server:
     - name: local_in
       type: http_expose
       listen: "[::]:8080"
       site_group: local
       escaper: default

The same group can also live in ``site-group.d/local.yaml`` when the main file
uses a directory path:

.. code-block:: yaml

   site_group: site-group.d
