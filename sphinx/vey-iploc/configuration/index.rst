.. _configuration:

#############
Configuration
#############

``vey-iploc`` uses YAML for configuration.

The main configuration file is specified with the ``-c`` command-line option.
If a directory is set, the main configuration file is loaded automatically from
it based on the executable binary name.
Its top-level keys are listed below.

At a high level, ``vey-iploc`` is organized around a few main object types:

* ``server`` accepts lookup datagrams on UDP or a Unix datagram socket
* ``geoip_db`` names the country and ASN database files used by those lookups

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
   * - geoip_db
     - Map
     - yes
     - Country and ASN database files, see :doc:`geoip`

Startup fails when ``server`` is missing or empty. ``geoip_db`` may be omitted.
Lookups then use empty country and ASN tables until a database is loaded.

.. _configuration_reload:

Reload
======

``runtime``, ``worker``, ``stat``, and ``controller`` are read only at startup.

``vey-iploc-ctl reload`` re-reads ``server`` and ``geoip_db`` from the config
file. A new or changed server is started, and a removed server is stopped. An
unchanged server keeps running. Country and ASN files named in the reloaded
config are read completely before they replace the tables used by lookups. A
database omitted from the reloaded config is left unchanged.

``vey-iploc-ctl reload-country`` and ``vey-iploc-ctl reload-asn`` each re-read
one database from the path already stored in this process. The file is read
completely before it replaces the table used by lookups. The other database is
left unchanged. These commands do not re-read the config file and do not
restart servers. Each command fails when that database path was never
configured.

``vey-iploc-ctl query <ip>`` looks up one address in the running tables and
prints the network, country, continent, ASN, and ISP fields that are set.

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

.. toctree::
   :hidden:

   runtime
   stat
   servers/index
   geoip
   values
