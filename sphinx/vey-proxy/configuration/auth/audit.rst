.. _configuration_auth_user_audit:

**********
User Audit
**********

.. versionadded:: 1.7.0

User-audit configuration is a map used to define audit behavior at the user
level.

enable_protocol_inspection
--------------------------

**optional**, **type**: bool

Controls whether protocol inspection is enabled.

Protocol inspection is enabled for a request only when this is ``true`` and
auditing is enabled on both the server side and the user side.

**default**: false

prohibit_unknown_protocol
-------------------------

**optional**, **type**: bool

Controls whether unknown protocols are blocked when protocol inspection is
enabled.

**default**: false

prohibit_timeout_protocol
-------------------------

**optional**, **type**: bool

We need to read the initial data to check the protocol type, and we can set the timeout value via the
:ref:`data0_read_timeout <conf_value_dpi_protocol_inspection_data0_read_timeout>` config option in
auditor :ref:`protocol inspection <conf_auditor_protocol_inspection>` config.

Controls whether the protocol should be blocked when inspection times out.

**default**: true

.. versionadded:: 1.9.1

task_audit_ratio
----------------

**optional**, **type**: :ref:`random ratio <conf_value_random_ratio>`

Sampling ratio for task-level auditing, such as ICAP ``REQMOD`` and
``RESPMOD``, on incoming user requests.

This setting also controls whether protocol inspection is actually enabled for a
specific user request.

If set, this overrides the
:ref:`task audit ratio <conf_auditor_task_audit_ratio>` configured on the
auditor.

**default**: not set, **alias**: application_audit_ratio

.. versionadded:: 1.7.4
