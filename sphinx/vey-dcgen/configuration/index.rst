.. _configuration:

#############
Configuration
#############

``vey-dcgen`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
If a directory is set, the main configuration file is loaded automatically from
it based on the executable binary name.
Its top-level keys are listed below.

At a high level, ``vey-dcgen`` is organized around a few main object types:

* ``server`` accepts certificate-request datagrams on UDP or a Unix datagram socket
* ``issuer`` names the CA certificate and private key used to issue certificates
* ``site`` holds predefined certificates selected by site id

Process-level settings such as ``runtime``, ``worker``, ``stat``, and
``controller`` control how the daemon runs and how it reports operational
state.

.. list-table::
   :header-rows: 1

   * - Key
     - Type
     - Reload
     - Description
   * - runtime
     - Map
     - no
     - Runtime configuration, see :doc:`runtime`
   * - worker
     - Map [#w]_
     - no
     - Starts unaided worker runtimes if present, see :doc:`runtime`
   * - stat
     - Map
     - no
     - Metrics configuration, see :doc:`stat`
   * - controller
     - Map
     - no
     - Local control socket timeouts. The optional ``local`` map accepts ``recv_timeout`` and ``send_timeout`` in seconds. Defaults are 30 and 1
   * - server
     - Mix [#m]_
     - yes
     - Listen sockets, see :doc:`servers/index`. At least one server is required at startup
   * - issuer
     - Map
     - yes
     - CA certificate and private key used to issue certificates. Required at startup. ``duration_stats`` configures the issue-duration histogram
   * - site
     - Mix [#m]_
     - yes
     - Predefined site certificates. Each map sets ``id``. Optional. ``sites`` is an alias. See :doc:`site`

Startup fails when ``server`` is missing or empty, or when ``issuer`` does
not name a CA certificate and private key.

.. _configuration_reload:

Reload
======

``runtime``, ``worker``, ``stat``, and ``controller`` are read only at startup.

``vey-dcgen-ctl reload`` re-reads ``server``, ``issuer``, and ``site`` from the
config file. A new or changed
server is started, and a removed server is stopped. An unchanged server keeps
running. Changing ``type`` for an existing name replaces that server. Changing
``listen`` on the same type restarts the listener and keeps its statistics.

The reloaded issuer replaces the CA certificate, private key, and the
``no_append_ca_cert``, ``keep_serial``, and ``max_ttl`` settings. The files
are read completely before the running CA is replaced. A request already
being signed keeps the previous CA. Omitting ``issuer`` leaves the running
CA unchanged. An issuer that fails to load leaves the running CA unchanged
and aborts the reload.

``site`` is read from the main config. Every map is parsed before the running
set is replaced. A map that fails to load aborts the reload and leaves the
running set unchanged. A site omitted from the new config is dropped.
Omitting ``site`` clears the running set.

``vey-dcgen-ctl reload-site <SITE-ID>`` re-reads the file recorded for that
site. The new map is applied only when its ``id`` is ``SITE-ID``. A site with
no recorded file cannot be reloaded this way. An unknown id is an error and
leaves the running set unchanged.

``vey-dcgen-ctl reload-issuer`` re-reads ``issuer`` from the main config.
Listeners keep running. The same issuer swap rules apply. Site certificates
are left as they are.

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

.. toctree::
   :hidden:

   runtime
   stat
   servers/index
   issuer
   site
   values
