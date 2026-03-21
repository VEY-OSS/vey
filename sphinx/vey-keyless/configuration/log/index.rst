.. _configuration_log:

***
Log
***

This section configures event logging. It is optional and cannot be reloaded.
If set, the root value must reside in the main configuration file.

Root Value
==========

The root value may be a simple string containing the driver name, for example:

- discard

  Drops logs. This is the **default**.

- journal

  Sends logs directly to journald.

- syslog

  Sends logs directly to syslog.

- stdout

  Sends logs to standard output.

In that form, the chosen driver becomes the default log configuration for all
loggers.

The root value may also be a map with the following keys:

- default

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Default log configuration for loggers with no explicit config.

  **default**: discard

- syslog

  **optional**, **type**: :ref:`syslog <configuration_log_driver_syslog>`

  Shared syslog driver configuration.

  **default**: not set

- fluentd

  **optional**, **type**: :ref:`fluentd <configuration_log_driver_fluentd>`

  Shared fluentd driver configuration.

  **default**: not set

- task

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Log configuration for task loggers.

  **default**: not set

.. _configuration_log_config:

Log Config Value
================

Each detailed log configuration may be a simple driver name or a map with the
following keys:

- journal

  **optional**, **type**: map

  Use the ``journal`` log driver. The map should be empty because no extra keys
  are currently defined.

- syslog

  **optional**, **type**: :ref:`syslog <configuration_log_driver_syslog>`

  Use the ``syslog`` log driver.

- fluentd

  **optional**, **type**: :ref:`fluentd <configuration_log_driver_fluentd>`

  Use the ``fluentd`` log driver.

- async_channel_size

  **optional**, **type**: usize

  Internal async channel size.

  **default**: 4096

- async_thread_number

  **optional**, **type**: usize

  Number of async threads.

  This has no effect on *discard* and *journal* log driver.

  **default**: 1

- io_error_sampling_offset

  **optional**, **type**: usize, **max**: 16

  The logger may encounter I/O errors. This controls how often they are
  reported: once every ``2^n`` occurrences.

  This has no effect on *discard* and *journal* log driver.

  **default**: 10

.. note:: The ``discard`` driver has no configuration options, so there is no
   corresponding map field for it.

.. _configuration_log_driver:

Drivers
=======

- discard
- stdout
- systemd journal
- :doc:`driver/syslog`
- :doc:`driver/fluentd`

.. toctree::
   :hidden:
   :glob:

   driver/*
