
.. _configure_rate_limit_value_types:

**********
Rate Limit
**********

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: not currently used
   - ``vey-statsd``: not currently used

.. _conf_value_tcp_sock_speed_limit:

tcp socket speed limit
======================

**yaml value**: mix

It consists of three fields:

* shift_millis | shift

  **type**: int

  Time-slice size in ``2^N`` milliseconds, where ``N`` is in the range
  ``[0, 12]``.
  For example, if ``N`` is ``10``, the time slice is ``1024ms``.
  If omitted, no limit is applied.

* upload | north | upload_bytes | north_bytes

  **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Maximum upload bytes allowed in each time slice. ``0`` means delay forever.

* download | south | download_bytes | south_bytes

  **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Maximum download bytes allowed in each time slice. ``0`` means delay forever.

The YAML value can be written in several forms:

* :ref:`humanize usize <conf_value_humanize_usize>`

  This will set upload and download to the same value, with shift_millis set to 10.

* map

  The keys of this map are the fields as described above.

Example:

.. code-block:: yaml

   tcp_sock_speed_limit:
     shift_millis: 10
     upload: 8MiB
     download: 32MiB

.. _conf_value_udp_sock_speed_limit:

udp socket speed limit
======================

**yaml value**: mix

It consists of five fields:

* shift_millis | shift

  **type**: int

  Time-slice size in ``2^N`` milliseconds, where ``N`` is in the range
  ``[0, 12]``.
  For example, if ``N`` is ``10``, the time slice is ``1024ms``.
  If omitted, no limit is applied.

* upload_bytes | north_bytes

  **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Maximum upload bytes allowed in each time slice. ``0`` means no byte limit.

* download_bytes | south_bytes

  **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Maximum download bytes allowed in each time slice. ``0`` means no byte limit.

* upload_packets | north_packets

  **type**: int [usize]

  Maximum upload packets allowed in each time slice. ``0`` means no packet limit.

* download_packets | south_packets

  **type**: int [usize]

  Maximum download packets allowed in each time slice. ``0`` means no packet limit.

The YAML value can be written in several forms:

* :ref:`humanize usize <conf_value_humanize_usize>`

  This will set upload and download bytes to the same value, set shift_millis to 10 and disable check on packets.

* map

  The keys of this map are the fields as described above.

Example:

.. code-block:: yaml

   udp_sock_speed_limit:
     shift: 10
     north_bytes: 8MiB
     south_bytes: 8MiB
     north_packets: 2000
     south_packets: 2000

.. _conf_value_global_stream_speed_limit:

global stream speed limit
=========================

**yaml value**: mix

It consists of three fields:

* replenish_interval

  **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Set the replenish interval value.

  **default**: 1s

* replenish_bytes

  **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Set the replenish byte size to add whenever ``replenish_interval`` elapses.

* max_burst_bytes

  **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Set the max byte size.

  **default**: the same as ``replenish_bytes``

The yaml value type can be in varies formats:

* :ref:`humanize usize <conf_value_humanize_usize>`

  This is the same as setting ``replenish_bytes`` to that value.

* map

  The keys of this map are the fields as described above, and
  ``replenish_bytes`` is always required.

.. availability::


   - ``vey-proxy``: available since ``1.9.6``

.. _conf_value_global_datagram_speed_limit:

global datagram speed limit
===========================

**yaml value**: mix

It consists of five fields:

* replenish_interval

  **optional**, **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Set the replenish interval value.

  **default**: 1s

* replenish_bytes

  **optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Set the replenish byte size to add whenever ``replenish_interval`` elapses.

  If not set, no bytes limitation will be applied.

* replenish_packets

  **optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Set the replenish packet count to add whenever ``replenish_interval`` elapses.

  If not set, no packets limitation will be applied.

* max_burst_bytes

  **optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Set the max byte size.

  **default**: the same as ``replenish_bytes``

* max_burst_packets

  **optional**, **type**: :ref:`humanize usize <conf_value_humanize_usize>`

  Set the max packet count.

  **default**: the same as ``replenish_packets``

The YAML value can be written in several forms:

* :ref:`humanize usize <conf_value_humanize_usize>`

  This is the same as setting ``replenish_bytes`` to that value.

* map

  The keys of this map are the fields as described above,
  and at least one of ``replenish_bytes`` or ``replenish_packets`` must be set.

.. availability::


   - ``vey-proxy``: available since ``1.9.6``

.. _conf_value_request_limit:

request limit
=============

**yaml value**: mix

It consists of two fields:

* shift_millis | shift

  **type**: int

  Time-slice size in ``2^N`` milliseconds, where ``N`` is in the range
  ``[0, 12]``.
  For example, if ``N`` is ``10``, the time slice is ``1024ms``.
  If omitted, no limit is applied.

* requests

  **type**: usize

  This sets the max requests in the time slice. 0 is not allowed.

.. _conf_value_rate_limit_quota:

rate limit quota
================

**yaml value**: mix

It consists of three fields:

* rate

  **type**: :ref:`nonzero u32 <conf_value_nonzero_u32>`

  If an integer or a string without a unit is used, the default unit is
  requests per second.

  Supported units for string values:

    - ``/s``, per second
    - ``/m``, per minute
    - ``/h``, per hour

* replenish_interval

  **type**: :ref:`humanize duration <conf_value_humanize_duration>`

  Construct a quota that replenishes one cell in a given interval. The default max_burst value is 1 is its not specified
  along with this option.

* max_burst

  Adjusts the maximum burst size for a quota to construct a rate limiter with a capacity
  for at most the given number of cells

.. note:: *rate* and *replenish_interval* is conflict with each other, the latter one in conf will take effect.

The yaml value for *u32 limit quota* can be in varies formats:

* simple rate

  Just the rate value. The max_burst value is the same as the one set in the rate.

* map

  The keys of this map are the fields as described above.

.. _conf_value_random_ratio:

random ratio
============

**yaml value**: f64 | str | bool | integer

Set a random ratio between 0.0 and 1.0 (inclusive).

For *str* value, it can be in fraction form (n/d), in percentage form (n%), or just a float string.

For *bool* value, *false* means 0.0, *true* means 1.0.

For *integer* value, only 0 and 1 is allowed.
