.. _configuration_runtime:

*******
Runtime
*******

The optional ``runtime`` section in ``vey-iploc`` uses the shared
:external+values:ref:`daemon runtime config <conf_value_daemon_runtime_config>`
value type.

The main runtime stays single-threaded. ``vey-iploc`` sets the default
``thread_number`` to ``0``, which selects a current-thread runtime, unless the
config sets another value.

If the optional ``worker`` section is present, it uses the shared
:external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`
value type.

``thread_number`` in that section is the worker count. Leave
``thread_number_per_runtime`` at its default of 1 so each worker runtime has
one thread.

UDP servers place one reuseport socket on each worker when at least one worker
is running. Without workers, each UDP server binds its sockets on the main
runtime. Unix servers always run on the main runtime.

See :external+values:doc:`configuration/values/runtime` for the full runtime
reference, including worker-thread settings, CPU-affinity-related value types,
and graceful-shutdown timers.
