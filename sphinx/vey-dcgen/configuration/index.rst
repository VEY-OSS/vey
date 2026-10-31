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
* ``backend`` names the CA certificate and private key used to issue certificates

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
   * - backend
     - Map
     - no
     - CA certificate and private key used to issue certificates. Required at startup

Startup fails when ``server`` is missing or empty, or when ``backend`` does
not name a CA certificate and private key.

.. _configuration_reload:

Reload
======

``runtime``, ``worker``, ``stat``, ``controller``, and ``backend`` are read
only at startup. Replacing the CA requires a new process.

``vey-dcgen-ctl reload`` re-reads ``server`` from the config file. A new or
changed server is started, and a removed server is stopped. An unchanged
server keeps running. Changing ``type`` for an existing name replaces that
server. Changing ``listen`` on the same type restarts the listener and keeps
its statistics.

.. rubric:: Footnotes

.. [#m] See :external+values:ref:`hybrid map <conf_value_hybrid_map>` for the actual format.
.. [#w] See :external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`.

.. toctree::
   :hidden:

   runtime
   stat
   servers/index
   backend
   values
