.. _configuration_runtime:

*******
Runtime
*******

The ``runtime`` section in ``vey-statsd`` uses the shared
:external+values:ref:`daemon runtime config <conf_value_daemon_runtime_config>`
value type.

If the optional ``worker`` section is present, it uses the shared
:external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`
value type.

See :external+values:doc:`configuration/values/runtime` for the full runtime
reference, including worker-thread settings, CPU-affinity-related value types,
and graceful-shutdown timers.
