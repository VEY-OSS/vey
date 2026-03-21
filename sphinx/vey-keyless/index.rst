##########################
``vey-keyless`` Reference
##########################

``vey-keyless`` is a server implementation of the Cloudflare Keyless SSL
protocol. It lets TLS termination infrastructure delegate private-key
operations to a separate service, which is useful when keys need to stay in a
controlled environment or behind hardware-backed crypto providers.

In a typical deployment, ``vey-keyless`` runs behind a TLS edge or gateway.
The edge service forwards keyless requests to this daemon, which loads the
requested private keys from configured stores and performs the cryptographic
operations on behalf of the edge.

The core configuration model is organized around a few main object types:

* ``server`` accepts incoming keyless protocol requests
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
