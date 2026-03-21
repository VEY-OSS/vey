.. _configuration_auditor:

*******
Auditor
*******

Each auditor configuration item is a map. The supported keys are described
below.

name
----

**required**, **type**: :external+values:ref:`metric node name <conf_value_metric_node_name>`

The auditor name. It can be referenced from
:ref:`server config <conf_server_common_auditor>`.

.. _conf_auditor_protocol_inspection:

protocol_inspection
-------------------

**optional**, **type**: :external+values:ref:`protocol inspection <conf_value_dpi_protocol_inspection>`

Basic protocol-inspection configuration.

**default**: set with default value

server_tcp_portmap
------------------

**optional**, **type**: :external+values:ref:`server tcp portmap <conf_value_dpi_server_tcp_portmap>`

Port mapping used for protocol inspection based on the server-side TCP port.

**default**: set with default value

client_tcp_portmap
------------------

**optional**, **type**: :external+values:ref:`client tcp portmap <conf_value_dpi_client_tcp_portmap>`

Port mapping used for protocol inspection based on the client-side TCP port.

**default**: set with default value

.. _conf_auditor_tls_cert_agent:

tls_cert_agent
--------------

**optional**, **type**: :external+values:ref:`tls cert agent <conf_value_dpi_tls_cert_agent>`

Certificate generator used for TLS interception.

If this field is not set, TLS interception is disabled.

**default**: not set, **alias**: tls_cert_generator

tls_ticketer
------------

**optional**, **type**: :external+values:ref:`tls ticketer <conf_value_tls_ticketer>`

Configures a remote rolling TLS ticketer.

**default**: not set

.. versionadded:: 1.9.9

.. _conf_auditor_tls_interception_client:

tls_interception_client
-----------------------

**optional**, **type**: :external+values:ref:`tls interception client <conf_value_dpi_tls_interception_client>`

TLS client configuration used for the upstream-side handshake during TLS
interception.

**default**: set with default value

tls_interception_server
-----------------------

**optional**, **type**: :external+values:ref:`tls interception server <conf_value_dpi_tls_interception_server>`

TLS server configuration used for the client-side handshake during TLS
interception.

**default**: set with default value

tls_stream_dump
---------------

**optional**, **type**: :external+values:ref:`stream dump <conf_value_dpi_stream_dump>`

Configures export of intercepted inner TLS streams to a remote service.

**default**: not set

.. versionadded:: 1.7.34

log_uri_max_chars
-----------------

**optional**, **type**: usize

Maximum number of URI characters retained in logs.

**default**: 1024

.. _conf_auditor_h1_interception:

h1_interception
---------------

**optional**, **type**: :external+values:ref:`h1 interception <conf_value_dpi_h1_interception>`

HTTP/1.x interception configuration.

**default**: set with default value

h2_inspect_policy
-----------------

**optional**, **type**: :external+values:ref:`protocol inspect policy <conf_value_dpi_protocol_inspect_policy>`

Controls how HTTP/2 traffic is handled.

**default**: intercept

.. versionadded:: 1.9.0

.. _conf_auditor_h2_interception:

h2_interception
---------------

**optional**, **type**: :external+values:ref:`h2 interception <conf_value_dpi_h2_interception>`

HTTP/2 interception configuration.

**default**: set with default value

websocket_inspect_policy
------------------------

**optional**, **type**: :external+values:ref:`protocol inspect policy <conf_value_dpi_protocol_inspect_policy>`

Controls how WebSocket traffic is handled.

**default**: intercept

.. versionadded:: 1.9.8

smtp_inspect_policy
-------------------

**optional**, **type**: :external+values:ref:`protocol inspect policy <conf_value_dpi_protocol_inspect_policy>`

Controls how SMTP traffic is handled.

**default**: intercept

.. versionadded:: 1.9.0

.. _conf_auditor_smtp_interception:

smtp_interception
-----------------

**optional**, **type**: :external+values:ref:`smtp interception <conf_value_dpi_smtp_interception>`

SMTP interception configuration.

**default**: set with default value

.. versionadded:: 1.9.2

imap_inspect_policy
-------------------

**optional**, **type**: :external+values:ref:`protocol inspect policy <conf_value_dpi_protocol_inspect_policy>`

Controls how IMAP traffic is handled.

**default**: intercept

.. versionadded:: 1.9.4

.. _conf_auditor_imap_interception:

imap_interception
-----------------

**optional**, **type**: :external+values:ref:`smtp interception <conf_value_dpi_imap_interception>`

IMAP interception configuration.

**default**: set with default value

.. versionadded:: 1.9.7

icap_reqmod_service
-------------------

**optional**, **type**: :external+values:ref:`icap service config <conf_value_audit_icap_service_config>`

ICAP ``REQMOD`` service configuration.

**default**: not set

.. versionadded:: 1.7.3

icap_respmod_service
--------------------

**optional**, **type**: :external+values:ref:`icap service config <conf_value_audit_icap_service_config>`

ICAP ``RESPMOD`` service configuration.

**default**: not set

.. versionadded:: 1.7.3

.. _conf_auditor_stream_detour_service:

stream_detour_service
---------------------

**optional**, **type**: :external+values:ref:`stream detour service config <conf_value_audit_stream_detour_service_config>`

Configuration for the :ref:`Stream Detour <protocol_helper_stream_detour>`
service.

To actually enable detouring, the inspect policy for the relevant protocol must
also be set to ``detour``.

If no stream detour service is configured here, protocols set to ``detour`` are
bypassed instead.

**default**: not set

.. versionadded:: 1.9.8

.. _conf_auditor_task_audit_ratio:

task_audit_ratio
----------------

**optional**, **type**: :external+values:ref:`random ratio <conf_value_random_ratio>`

Sampling ratio for task-level auditing, such as ICAP ``REQMOD`` and
``RESPMOD``, on incoming requests.

This setting also controls whether protocol inspection is actually enabled for a
given request.

User-side settings may override this value.

**default**: 1.0, **alias**: application_audit_ratio

.. versionadded:: 1.7.4
