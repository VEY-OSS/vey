.. _configuration_log:

***
Log
***

This is the optional configuration section for structured event logs. It
cannot be reloaded. If present, the root value described below must appear in
the main configuration file.

Root Value
==========

The root value may be a simple string naming the log driver, for example:

- discard

  Drop all logs. This is the **default**.

- journal

  Send logs directly to ``journald``.

- syslog

  Send logs directly to ``syslog``.

- stdout

  Write logs to standard output.

  .. versionadded:: 0.3.5

In this form, the selected driver becomes the default configuration for all
loggers.

The root value may also be a map with the following keys:

- default

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Set the default log configuration for loggers without an explicit override.

  **default**: discard

- syslog

  **optional**, **type**: :ref:`syslog <configuration_log_driver_syslog>`

  Define the reusable ``syslog`` driver configuration.

  **default**: not set

  .. versionadded:: 0.3.7

- fluentd

  **optional**, **type**: :ref:`fluentd <configuration_log_driver_fluentd>`

  Define the reusable ``fluentd`` driver configuration.

  **default**: not set

  .. versionadded:: 0.3.7

- task

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Set the log configuration for ``task`` loggers.

  **default**: not set

.. _configuration_log_config:

Log Config Value
================

Each detailed logger configuration may be a simple driver name or a map with
the following keys:

- journal

  **optional**, **type**: map

  Use the ``journal`` driver. The map is currently empty because no
  driver-specific keys are defined.

- syslog

  **optional**, **type**: :ref:`syslog <configuration_log_driver_syslog>`

  Use the ``syslog`` driver.

- fluentd

  **optional**, **type**: :ref:`fluentd <configuration_log_driver_fluentd>`

  Use the ``fluentd`` driver.

- async_channel_size

  **optional**, **type**: usize

  Set the internal async channel size.

  **default**: 4096

- async_thread_number

  **optional**, **type**: usize

  Set the number of asynchronous worker threads.

  This has no effect on the ``discard`` and ``journal`` drivers.

  **default**: 1

- io_error_sampling_offset

  **optional**, **type**: usize, **max**: 16

  I/O errors can occur while writing logs. This setting controls sampling by
  logging one error out of every ``2^n`` occurrences.

  This has no effect on the ``discard`` and ``journal`` drivers.

  **default**: 10

.. note:: The ``discard`` driver has no configuration fields, so it does not
   have a corresponding map entry.

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
