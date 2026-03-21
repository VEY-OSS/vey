.. _log:

###
Log
###

Multiple logging drivers are supported. See :ref:`log <configuration_log>` for
driver configuration details.

All emitted logs are structured. This section documents the fields used by each
log type.

Shared Keys
===========

The following shared keys are present in all log types:

daemon_name
-----------

**optional**, **type**: string

Daemon group name of the process, as configured by the config file or command-line options.

pid
---

**required**, **type**: int

The pid of the process.

There may be many processes running, one online and the others in offline mode.

log_type
--------

**required**, **type**: enum string

Log type. The meaning of non-shared keys depends on this value.

Values are:

  * Task
  * Request

.. _log_shared_keys_report_ts:

report_ts
---------

**optional**, **type**: unix timestamp

Timestamp when the log entry was generated.

This field is present when the selected log driver is configured to append it.

Log Types
=========

.. toctree::
   :maxdepth: 1

   task
   request
