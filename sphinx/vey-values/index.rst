vey-values Documentation
========================

``vey-values`` is the shared configuration value reference used by the VEY
application documentation sets. It collects common value types such as network
addresses, TLS objects, ACL rules, metrics types, rate-limit settings, runtime
settings, and other reusable configuration fragments.

Application-specific docs should link here when they reference ``conf_value_*``
labels rather than documenting duplicate copies in each project tree.

This shared project also tracks application availability so readers can tell
whether a common value type, or a newer addition to one, is supported by a
specific VEY application release.

.. toctree::
   :maxdepth: 1

   configuration/values/index
