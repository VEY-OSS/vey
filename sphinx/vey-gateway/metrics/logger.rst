.. _metrics_logger:

##############
Logger Metrics
##############

These metrics are emitted by loggers that support metrics.

The ``discard`` and ``journal`` drivers do not emit metrics. See
:ref:`log <configuration_log>` for the logging configuration model.

The following tags are present on all logger metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* logger

  The logger name.

The metrics are:

* logger.message.total

  **type**: count

  The total number of log messages received by the logger.

* logger.message.pass

  **type**: count

  The number of log messages successfully sent to the downstream target.

* logger.traffic.pass

  **type**: count

  The number of log payload bytes successfully sent to the downstream target.

* logger.message.drop

  The number of log messages dropped.

  An extra ``drop_type`` tag gives the drop reason. Values are:

  - ``FormatFailed``: the message could not be encoded for the selected log protocol
  - ``ChannelClosed``: the internal async channel was closed
  - ``ChannelOverflow``: the internal async channel was full
  - ``PeerUnreachable``: the downstream peer was closed or unreachable
