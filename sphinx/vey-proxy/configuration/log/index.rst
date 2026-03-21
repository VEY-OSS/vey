.. _configuration_log:

***
Log
***

This section describes event-log configuration. It is optional and cannot be
reloaded. If present, the root value must be defined in the main configuration
file.

Root Value
==========

The root value can be a simple string naming the log driver, for example:

- discard

  Drop all logs. This is the **default**.

- journal

  Send logs directly to journald.

- syslog

  Send logs directly to syslogd.

- stdout

  Send logs to standard output.

  .. versionadded:: 1.9.8

In this form, the selected driver becomes the default configuration for all
loggers.

The root value can also be a map with the following keys:

- default

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Default log configuration for loggers without an explicit override.

  **default**: discard

- syslog

  **optional**, **type**: :ref:`syslog <configuration_log_driver_syslog>`

  Shared ``syslog`` driver configuration.

  **default**: not set

  .. versionadded:: 1.11.0

- fluentd

  **optional**, **type**: :ref:`fluentd <configuration_log_driver_fluentd>`

  Shared ``fluentd`` driver configuration.

  **default**: not set

  .. versionadded:: 1.11.0

- task

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Log configuration for *task* loggers.

  **default**: not set

- escape

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Log configuration for *escape* loggers.

  **default**: not set

- resolve

  **optional**, **type**: :ref:`log config <configuration_log_config>`

  Log configuration for *resolve* loggers.

  **default**: not set

.. _configuration_log_config:

Log Config Value
================

Each detailed log configuration can be either a simple driver name or a map
with the following keys:

- journal

  **optional**, **type**: map

  Use the ``journal`` log driver. The map is currently empty because there are
  no driver-specific keys.

- syslog

  **optional**, **type**: :ref:`syslog <configuration_log_driver_syslog>`

  Use the ``syslog`` log driver.

- fluentd

  **optional**, **type**: :ref:`fluentd <configuration_log_driver_fluentd>`

  Use the ``fluentd`` log driver.

- async_channel_size

  **optional**, **type**: usize

  Size of the internal async channel.

  **default**: 4096

- async_thread_number

  **optional**, **type**: usize

  Number of async worker threads.

  This setting has no effect for the ``discard`` and ``journal`` drivers.

  **default**: 1

- io_error_sampling_offset

  **optional**, **type**: usize, **max**: 16

  Loggers may encounter repeated I/O errors. This setting controls exponential
  sampling of those errors: an error is logged every ``2^n`` occurrences, where
  ``n`` is the configured value.

  This setting has no effect for the ``discard`` and ``journal`` drivers.

  **default**: 10

.. note:: The ``discard`` driver has no configuration options, so it has no
   corresponding map field.

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
