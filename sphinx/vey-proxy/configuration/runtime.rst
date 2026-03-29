.. _configuration_runtime:

*******
Runtime
*******

The optional ``runtime`` section in ``vey-proxy`` uses the shared
:external+values:ref:`daemon runtime config <conf_value_daemon_runtime_config>`
value type.

If the optional ``worker`` section is present, it uses the shared
:external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`
value type.

See :external+values:doc:`configuration/values/runtime` for the full runtime
reference, including worker-thread settings, CPU-affinity-related value types,
and graceful-shutdown timers.

Both sections are process-level settings loaded only from the main
configuration file. They are skipped during hot reload.

.. note::

   In ``vey-proxy``, ``server_offline_delay`` defaults to ``8s`` starting from
   version ``1.7.25``. Earlier versions used ``4s``.
