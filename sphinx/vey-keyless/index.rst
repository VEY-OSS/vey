##########################
``vey-keyless`` Reference
##########################

``vey-keyless`` is a dedicated private-key service for TLS deployments. Its
primary key-operation protocol is Cloudflare Keyless SSL, and listening sockets
can be composed in front of that protocol through pluggable server types.

It is intended for deployments where TLS private-key operations should be
handled by a dedicated service rather than by the edge process that terminates
client connections. This makes it easier to centralize key handling, integrate
with hardware acceleration, and keep private-key access under tighter control.

In a typical deployment, ``vey-keyless`` runs behind a TLS edge or gateway.
Connections reach a listening server — either a ``cloudflare`` key-operation
server directly, or a port server such as ``plain_tls_port`` /
``plain_tcp_port`` that forwards the stream — and private-key operations are
performed against keys loaded from configured stores.

The core configuration model is organized around a few main object types:

* ``server`` accepts incoming connections. A server either speaks a
  key-operation protocol (``cloudflare``, the default) or only owns a listening
  socket and forwards every accepted connection to a next server
  (``plain_tcp_port``, ``plain_tls_port``)
* ``store`` defines where private keys are loaded from
* ``backend`` defines how signing or decryption work is executed
* ``log`` and ``stat`` define operational visibility

This reference is organized into three sections:

* :doc:`configuration/index` documents all configuration objects and their
  relationships
* :doc:`metrics/index` documents the exported StatsD metrics
* :doc:`log/index` documents the structured log formats emitted by the daemon

If you are setting up ``vey-keyless`` for the first time, start with the
configuration reference, then review the metrics and log pages for operational
visibility.

.. toctree::
   :maxdepth: 1

   Configuration Reference <configuration/index>
   Metrics Definition <metrics/index>
   Log Format <log/index>
