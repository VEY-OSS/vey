.. _configuration_exporter_influxdb_v3:

influxdb_v3
===========

Exporter that writes InfluxDB line protocol with ``POST /api/v3/write_lp``.
The ``db`` parameter names the database, and the token is sent as ``Authorization: Bearer``.
InfluxDB 3 accepts this path over cleartext HTTP.

Configuration
-------------

The following common keys are supported:

* :ref:`prefix <conf_exporter_common_prefix>`
* :ref:`global_tags <conf_exporter_common_global_tags>`

The :ref:`HTTP Export Runtime <configuration_exporter_runtime_http>` is used:

- default port 8181
- all config keys supported

emit_interval
^^^^^^^^^^^^^

**optional**, **type**: :external+values:ref:`humanize duration <conf_value_humanize_duration>`

Emit interval for outgoing batches.

**default**: 10s

database
^^^^^^^^

**required**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Database name. Sent as the ``db`` query parameter.

token
^^^^^

**optional**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Authentication token. Sent as ``Authorization: Bearer <token>``.

If not set, the value in environment variable ``INFLUXDB3_AUTH_TOKEN`` is used.

**default**: not set

precision
^^^^^^^^^

**optional**, **type**: string

Precision query parameter.

Allowed values are:

- second
- millisecond
- microsecond
- nanosecond

**default**: second

no_sync
^^^^^^^

**optional**, **type**: bool

Controls the ``no_sync`` query parameter.

**default**: false

max_body_lines
^^^^^^^^^^^^^^

**optional**, **type**: usize

Maximum number of line-protocol records sent in a single request.

**default**: 10000

Metric types
------------

Counters
^^^^^^^^

Each counter is one line. The measurement name is ``<name>``, and the line has three fields:

* ``count`` — cumulative sum
* ``diff`` — number of events in that ``emit_interval``
* ``rate`` — ``diff / emit_interval``, in events per second

::

    requests,host=a count=100,diff=7,rate=0.7 <timestamp>

A counter that keeps receiving increments of ``0`` stays in the export. ``count`` is written again with a new timestamp, and ``diff`` and ``rate`` are ``0``. A counter that received no samples during an ``emit_interval`` is omitted for that interval, and the sum starts again on the next sample. Counts from an omitted interval are gone.

Gauges
^^^^^^

Each gauge is one line with a single ``value`` field:

::

    requests,host=a value=3 <timestamp>

Backends
--------

InfluxDB 3 accepts this stream on the `v3 write_lp API`_. The exporter speaks cleartext HTTP, so the peer must accept cleartext on ``/api/v3/write_lp``.

.. _v3 write_lp API: https://docs.influxdata.com/influxdb3/enterprise/write-data/api-client-libraries/

InfluxDB
^^^^^^^^

The write path is ``/api/v3/write_lp``. Port 8181 matches this exporter's default.

``rate`` is already events per second. ``diff`` is the count for that ``emit_interval``. ``count`` is the running total. ``derivative`` on ``count`` goes down when the sum starts again.

::

    SELECT time, rate FROM requests WHERE time >= now() - INTERVAL '5 minutes'

Plot a gauge's ``value`` column.
