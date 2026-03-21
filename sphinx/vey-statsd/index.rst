#########################
``vey-statsd`` Reference
#########################

``vey-statsd`` is a StatsD-compatible metrics ingestion and forwarding service.
It can accept incoming StatsD traffic, apply collection or normalization
logic, and export the resulting metrics to one or more downstream systems.

Within the Vey project, it is used as a dedicated metrics pipeline component
between application daemons and downstream storage or observability systems.
The same pipeline design also makes it suitable as a standalone relay for
StatsD-compatible metrics.

At a high level, ``vey-statsd`` is built around three object families:

* importers, which receive metrics from upstream senders
* collectors, which aggregate or rewrite incoming metrics
* exporters, which emit the resulting metrics to downstream systems

This documentation currently focuses on the static configuration model used to
define those pipeline stages and the runtime settings around them.

The reference is organized as follows:

* :doc:`configuration/index` documents importers, collectors, exporters,
  runtime settings, and the shared value-type references imported from
  ``vey-values``.

If you are new to ``vey-statsd``, a practical reading order is:

1. start with :doc:`configuration/index`
2. choose an :doc:`importer <configuration/importer/index>`
3. choose one or more :doc:`collectors <configuration/collector/index>`
4. choose one or more :doc:`exporters <configuration/exporter/index>`

.. toctree::
   :maxdepth: 1

   Configuration Reference <configuration/index>
