
.. _configure_quic_value_types:

****
QUIC
****

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: not currently used
   - ``vey-statsd``: not currently used

.. _conf_value_quinn_transport:

Quinn Transport
===============

**yaml value**: map

Transport configuration used with Quinn.

The loader requires a map. An empty map is valid and still inherits Quinn's
builder defaults.

The map supports the following fields:

* max_idle_timeout

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Maximum allowed period of inactivity before the connection times out.
  The effective idle timeout is the minimum of this value and the peer's own
  maximum idle timeout.

  **default**: 60s

* keep_alive_interval

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Period of inactivity before sending a keepalive packet.
  To be effective, it must be lower than the idle timeout of both peers.

  **default**: 10s

* stream_receive_window

  **optional**, **type**: :ref:`humanize u32 <conf_value_humanize_u32>`

  Maximum number of bytes the peer may transmit on a single stream without
  acknowledgment before becoming blocked.
  This should be at least the expected connection latency multiplied by the
  maximum desired throughput.

  **default**: quinn default value

* receive_window

  **optional**, **type**: :ref:`humanize u32 <conf_value_humanize_u32>`

  Maximum number of bytes the peer may transmit across all streams in a
  connection without acknowledgment before becoming blocked.
  This should be at least the expected connection latency multiplied by the
  maximum desired throughput.

  **default**: quinn default value

* send_window

  **optional**, **type**: :ref:`humanize u32 <conf_value_humanize_u32>`

  Maximum number of bytes that may be transmitted to a peer without
  acknowledgment.

  **default**: quinn default value

Example:

.. code-block:: yaml

   transport:
     max_idle_timeout: 30s
     keep_alive_interval: 8s
     stream_receive_window: 1MiB
     receive_window: 8MiB
     send_window: 8MiB

.. availability::


   - ``vey-proxy``: available since ``1.9.9``
