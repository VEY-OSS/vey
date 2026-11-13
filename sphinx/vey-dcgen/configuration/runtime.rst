.. _configuration_runtime:

*******
Runtime
*******

The optional ``runtime`` section in ``vey-dcgen`` uses the shared
:external+values:ref:`daemon runtime config <conf_value_daemon_runtime_config>`
value type.

The main runtime is a current-thread runtime. ``thread_number`` is fixed at
``0``. The ``runtime`` section still accepts the other daemon runtime keys,
including graceful-shutdown timers. A ``thread_number`` other than ``0`` is
ignored, and a warning is logged.

If the optional ``worker`` section is present, it uses the shared
:external+values:ref:`unaided runtime config <conf_value_unaided_runtime_config>`
value type.

``thread_number`` in that section is the worker count. Leave
``thread_number_per_runtime`` at its default of 1 so each worker runtime has
one thread.

Each thread keeps its own issuer. ``load`` installs one on the main runtime
when no worker is running, and one on each worker thread otherwise. UDP
servers place one reuseport socket on each worker when at least one worker is
running, and that worker signs on its own thread. Without workers, each UDP
server binds its sockets on the main runtime and signs there. Unix servers
always accept on the main runtime. With workers, each Unix request is signed
on a worker thread. Without workers, it is signed on the main runtime thread.

See :external+values:doc:`configuration/values/runtime` for the full runtime
reference, including worker-thread settings, CPU-affinity-related value types,
and graceful-shutdown timers.
