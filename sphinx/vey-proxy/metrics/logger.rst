.. _metrics_logger:

##############
Logger Metrics
##############

Logger metrics are available for log drivers that expose metric reporting.

The ``discard`` and ``journal`` log drivers do not support metrics. See
:ref:`log <configuration_log>` for configuration details.

The following tags are present on all logger metrics:

* :ref:`daemon_group <metrics_tag_daemon_group>`
* :ref:`stat_id <metrics_tag_stat_id>`

* logger

  The logger name.

The metric names are:

* logger.message.total

  **type**: count

  Total number of log records processed.

* logger.message.pass

  **type**: count

  Number of log records successfully passed to the next peer.

* logger.traffic.pass

  **type**: count

  Total bytes of log data successfully sent to the next peer.

* logger.message.drop

  Number of log records dropped.

  An extra ``drop_type`` tag provides the drop reason. Supported values are:

  - ``FormatFailed``: the message could not be formatted for the target log
    protocol

  - ``ChannelClosed``: the internal async channel was closed

  - ``ChannelOverflow``: the internal async channel was full

  - ``PeerUnreachable``: the next peer was closed or temporarily unreachable
