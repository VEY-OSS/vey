##########################
``vey-gateway`` Reference
##########################

``vey-gateway`` is the gateway component in the Vey stack. It accepts
stream and keyless traffic on the edge, applies the configured server
behavior, and forwards requests to backend services discovered through the
configured routing layer.

Its configuration model is centered on a small set of cooperating objects:

* ``server`` accepts client traffic and defines frontend behavior
* ``discover`` resolves or expands upstream targets
* ``backend`` connects to upstream services and handles forwarding
* ``log`` and ``stat`` provide observability

This documentation is organized into three reference sections:

* :doc:`configuration/index` describes all configuration objects and their
  relationships.
* :doc:`metrics/index` documents the exported StatsD metrics.
* :doc:`log/index` describes the structured log formats emitted by the daemon.

For a first read, start with the configuration reference and then move to the
metrics and log sections to understand how the daemon reports runtime state and
task activity in production.

.. toctree::
   :maxdepth: 1

   Configuration Reference <configuration/index>
   Metrics Definition <metrics/index>
   Log Format <log/index>
