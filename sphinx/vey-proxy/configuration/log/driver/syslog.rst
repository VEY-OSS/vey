.. _configuration_log_driver_syslog:

syslog
======

The ``syslog`` driver configuration is a map.

It can send logs to syslogd over either:

 * a UNIX socket, which is the default
 * a UDP socket

The message format can be either:

 * ``rfc3164``, which is the default
 * ``rfc5424``

The supported keys are described below.

Example:

.. code-block:: yaml

   syslog:
     target:
       udp:
         address: 192.0.2.10:514
         bind_ip: 192.0.2.5
     format_rfc5424:
       enterprise_id: 32473
       message_id: vey-proxy
     emit_hostname: true

target_unix
-----------

**optional**, **type**: mix

Use this to send syslog messages to a custom UNIX-domain socket path.

The value can be a map, with the following keys:

* path

  **required**, **type**: :external+values:ref:`absolute path <conf_value_absolute_path>`

  Syslog daemon socket path.

If the value type is str, the value should be the same as the value as *path* above.

**default**: not set

target_udp
----------

**optional**, **type**: mix

Use this to send syslog messages to a remote syslog daemon listening on UDP.

The value can be a map, with the following keys:

* address

  **required**, **type**: :external+values:ref:`env sockaddr str <conf_value_env_sockaddr_str>`

  Remote socket address.

* bind_ip

  **optional**, **type**: :external+values:ref:`ip addr str <conf_value_ip_addr_str>`

  Local IP address to bind before sending.

  **default**: not set

If the value type is str, the value should be the same as the value as *address* above.

**default**: not set

target
------

**optional**, **type**: map

Alternative form for specifying the syslog target.

The key *udp* is just handled as *target_udp* as above.

The key *unix* is just handled as *target_unix* as above.

format_rfc5424
--------------

**optional**, **type**: mix

Enables RFC 5424 message format.

The value can be a map, with the following keys:

* enterprise_id

  **optional**, **type**: int

  Enterprise ID as defined in `rfc5424`_.

  See `PRIVATE ENTERPRISE NUMBERS`_ for IANA allocated numbers.

  **default**: 0, which is reserved

  .. _rfc5424: https://tools.ietf.org/html/rfc5424
  .. _PRIVATE ENTERPRISE NUMBERS: https://www.iana.org/assignments/enterprise-numbers/enterprise-numbers

* message_id

  **optional**, **type**: str

  Message ID.

  **default**: not set

If the value type is int, the value should be the same as the value as *enterprise_id* above.
If the value type is str, the value should be the same as the value as *message_id* above.

**default**: not set

use_cee_log_syntax
------------------

**optional**, **type**: bool

Controls whether `CEE Log Syntax`_ is used.

Enable this option if you need to use rsyslog `mmjsonparse`_ module.

**default**: not set

.. _mmjsonparse: https://www.rsyslog.com/files/temp/doc-indent/configuration/modules/mmjsonparse.html
.. _CEE Log Syntax: https://cee.mitre.org/language/1.0-beta1/cls.html

cee_event_flag
--------------

**optional**, **type**: ascii string

Custom CEE event-flag value. This is meaningful only when
``use_cee_log_syntax`` is enabled.

The one defined by `CLT`_ is *@cee:*, you can override it by using this option.

**default**: @cee:

.. _CLT: https://cee.mitre.org/language/1.0-beta1/clt.html

emit_hostname
-------------

**optional**, **type**: bool

Controls whether the hostname is included in the syslog message header.

**default**: false

append_report_ts
----------------

**optional**, **type**: bool

Controls whether :ref:`report_ts <log_shared_keys_report_ts>` is appended to
logs.

**default**: false
