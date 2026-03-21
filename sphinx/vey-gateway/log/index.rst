.. _log:

###
Log
###

``vey-gateway`` supports multiple logging drivers. See
:ref:`log <configuration_log>` for configuration details.

All emitted logs are structured. This section documents the fields used by
each log type.

Shared Keys
===========

The following shared keys are present in all log records:

daemon_name
-----------

**optional**, **type**: string

The daemon-group name for the process, configured in the config file or with
command-line options.

pid
---

**required**, **type**: int

The process ID of the emitting daemon.

More than one process may exist during graceful restart, with only one serving
traffic and others draining.

log_type
--------

**required**, **type**: enum string

The top-level log type. The meaning of non-shared keys depends on this value.

Values are:

  * Task

.. _log_shared_keys_report_ts:

report_ts
---------

**optional**, **type**: unix timestamp

The timestamp when the log record was generated.

This field is present only when the selected log driver is configured to
append it. See :ref:`log driver <configuration_log_driver>` for details.

Log Types
=========

.. toctree::
   :maxdepth: 1

   task/index
