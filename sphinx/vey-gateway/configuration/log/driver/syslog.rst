.. _configuration_log_driver_syslog:

syslog
======

The ``syslog`` driver configuration is a map.

Use this driver to send logs to a syslog service listening on:

* a Unix socket, which is the default
* a UDP socket

The message format can be:

* RFC 3164, which is the default
* RFC 5424

The keys are described below.

target_unix
-----------

**optional**, **type**: mix

Set this when you want to send syslog messages to a custom Unix socket path.

The value can be a map, with the following keys:

* path

  **required**, **type**: :external+values:ref:`absolute path <conf_value_absolute_path>`

  The syslog daemon listening socket path.

If the value is a string, it is interpreted the same way as the ``path`` field
above.

**default**: not set

target_udp
----------

**optional**, **type**: mix

Set this when you want to send syslog messages to a remote syslog daemon over
UDP.

The value can be a map, with the following keys:

* address

  **required**, **type**: :external+values:ref:`env sockaddr str <conf_value_env_sockaddr_str>`

  Set the remote socket address.

* bind_ip

  **optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

  Set the local IP address used for the outbound socket.

  **default**: not set

If the value is a string, it is interpreted the same way as the ``address``
field above.

**default**: not set

target
------

**optional**, **type**: map

This is an alternative form for defining the syslog target.

The key ``udp`` is handled the same way as ``target_udp`` above.

The key ``unix`` is handled the same way as ``target_unix`` above.

format_rfc5424
--------------

**optional**, **type**: mix

Set this to emit RFC 5424 messages.

The value can be a map, with the following keys:

* enterprise_id

  **optional**, **type**: int

  Set the enterprise ID described in `rfc5424`_.

  See `PRIVATE ENTERPRISE NUMBERS`_ for IANA allocated numbers.

  **default**: 0, which is reserved

  .. _rfc5424: https://tools.ietf.org/html/rfc5424
  .. _PRIVATE ENTERPRISE NUMBERS: https://www.iana.org/assignments/enterprise-numbers/enterprise-numbers

* message_id

  **optional**, **type**: str

  Set the message id.

  **default**: not set

If the value is an integer, it is interpreted the same way as
``enterprise_id`` above.
If the value is a string, it is interpreted the same way as ``message_id``
above.

**default**: not set

use_cee_log_syntax
------------------

**optional**, **type**: bool

Set whether to use `CEE Log Syntax`_.

Enable this if you need the rsyslog `mmjsonparse`_ module.

**default**: not set

.. _mmjsonparse: https://www.rsyslog.com/files/temp/doc-indent/configuration/modules/mmjsonparse.html
.. _CEE Log Syntax: https://cee.mitre.org/language/1.0-beta1/cls.html

cee_event_flag
--------------

**optional**, **type**: ascii string

Set a custom CEE event flag. This is meaningful only when
``use_cee_log_syntax`` is enabled.

The flag defined by `CLT`_ is ``@cee:``. Override it here if needed.

**default**: @cee:

.. _CLT: https://cee.mitre.org/language/1.0-beta1/clt.html

emit_hostname
-------------

**optional**, **type**: bool

Set whether to include the hostname in the syslog header.

**default**: false

append_report_ts
----------------

**optional**, **type**: bool

Set whether to append :ref:`report_ts <log_shared_keys_report_ts>` to log
records.

**default**: false
