.. _configuration_exporter_influxdb_v2:

influxdb_v2
===========

Exporter that writes InfluxDB line protocol with ``POST /api/v2/write``.
The ``bucket`` parameter names the database, and the token is sent as ``Authorization: Token``.
InfluxDB 2, InfluxDB 3, VictoriaMetrics, QuestDB, and OpenGemini accept this path.

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

bucket
^^^^^^

**required**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Bucket name. Sent as the ``bucket`` query parameter.

token
^^^^^

**optional**, **type**: :external+values:ref:`http header value <conf_value_http_header_value>`

Authentication token. Sent as ``Authorization: Token <token>``.

If not set, the value in environment variable ``INFLUX_TOKEN`` is used.

**default**: not set

precision
^^^^^^^^^

**optional**, **type**: string

Precision query parameter.

Allowed values are:

- s
- ms
- us
- ns

**default**: s

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

InfluxDB 2, InfluxDB 3, VictoriaMetrics, QuestDB, and OpenGemini accept ``POST /api/v2/write``.

GreptimeDB writes this protocol at ``/v1/influxdb/api/v2/write``. TDengine writes InfluxDB line protocol at ``/influxdb/v1/write``. This exporter always posts ``/api/v2/write``.

InfluxDB
^^^^^^^^

InfluxDB 2 serves the `v2 write API`_ on port 8086. Set ``port`` to match. The ``bucket`` key is the bucket name.

Keep ``idle_timeout`` shorter than ``--http-idle-timeout`` (default 3m). That option closes an idle HTTP connection.

.. _v2 write API: https://docs.influxdata.com/influxdb/v2/write-data/developer-tools/api/

``rate`` is already events per second. ``diff`` is the count for that ``emit_interval``. ``count`` is the running total. ``derivative`` on ``count`` goes down when the sum starts again.

::

    from(bucket: "example")
      |> range(start: -5m)
      |> filter(fn: (r) => r._measurement == "requests" and r._field == "rate")

Plot a gauge's ``value`` field.

InfluxDB 3 also serves ``/api/v2/write`` on port 8181. Set ``bucket`` to the database name. Read the fields with SQL, as on the v3 write API: ``rate`` is events per second, and ``count`` is the running total.

QuestDB
^^^^^^^

QuestDB accepts ``/api/v2/write`` on port 9000. Set ``port`` to ``9000``. ``bucket`` is required here and ignored by QuestDB. ``precision=s`` matches QuestDB. The open-source server does not check ``Authorization: Token``.

The measurement becomes a table. ``count``, ``diff``, and ``rate`` are columns. ``rate`` is already events per second, including after ``count`` starts again from zero:

::

    SELECT timestamp, rate FROM requests
    WHERE timestamp > dateadd('m', -5, now())

Plot a gauge's ``value`` column. See the `QuestDB ILP overview`_.

.. _QuestDB ILP overview: https://questdb.com/docs/ingestion/ilp/overview/

OpenGemini
^^^^^^^^^^

OpenGemini accepts ``/api/v2/write`` on port 8086. Set ``port`` to ``8086``. With authentication disabled, leave ``token`` unset and do not export ``INFLUX_TOKEN``. Enabled authentication expects Basic or JWT credentials, which this exporter does not send.

``rate``, ``diff``, and ``count`` are the same fields as on InfluxDB. Read ``rate`` for events per second:

::

    SELECT rate FROM requests WHERE time > now() - 5m

Plot a gauge's ``value`` field.

VictoriaMetrics
^^^^^^^^^^^^^^^

VictoriaMetrics accepts this write on ``/api/v2/write``. Point ``server`` and ``port`` at the HTTP listener. Single-node uses port 8428, and vmagent uses port 8429. See the `VictoriaMetrics InfluxDB integration`_.

Keep ``idle_timeout`` shorter than ``-http.idleConnTimeout`` (default 1m). That option closes an idle HTTP connection.

.. _VictoriaMetrics InfluxDB integration: https://docs.victoriametrics.com/victoriametrics/integrations/influxdb/

Leave ``token`` unset and do not export ``INFLUX_TOKEN``, unless that listener expects ``Authorization: Token``. An empty ``token`` still reads that variable.

Each field becomes its own series named ``<measurement>_<field>``. A counter is ``requests_count``, ``requests_diff``, and ``requests_rate``. A gauge is ``requests_value``. ``-influxSkipSingleField`` drops the suffix when a line has only one field, so a gauge can be stored as ``requests``.

Set a distinct ``global_tags`` value per sender, such as ``host``, so each machine stays on its own series.

``requests_count`` is the cumulative sum. Query its per-second rate in MetricsQL with ``rate()`` on each series, then sum:

::

    sum(rate(requests_count[5m]))

``rate()`` treats a decrease as a counter reset. After one machine restarts, its sum starts from zero and the old total is left out of the rate. The summed rate dips by that machine's own traffic. Applying ``rate()`` to the sum of the raw series treats the reset as a reset of the total, and the rate jumps.

``requests_rate`` is already events per second. ``rate()`` on it does not repeat that calculation. Repeated increments of ``0`` hold ``requests_count`` flat, so ``rate()`` on that series is ``0``.

The cluster vminsert path is ``/insert/<tenant>/influx/...``. This exporter sends ``/api/v2/write``.
