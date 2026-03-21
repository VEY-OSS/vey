
.. _configure_base_value_types:

****
Base
****

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: available
   - ``vey-keyless``: available
   - ``vey-statsd``: available

.. _conf_value_env_var:

env var
=======

**yaml value**: str

Environment-variable reference in the form ``$`` followed by the variable name,
for example ``$TCP_LISTEN_ADDR``.

The referenced environment-variable value is parsed exactly as if it had been
written directly in the YAML file as a string.

.. _conf_value_nonzero_u32:

nonzero u32
===========

**yaml value**: int

A non-zero ``u32`` value.

.. _conf_value_nonzero_usize:

nonzero usize
=============

**yaml value**: int

A non-zero ``usize`` value.

.. _conf_value_humanize_usize:

humanize usize
==============

**yaml value**: int | str

String values support binary units such as ``KiB`` and ``MiB`` as well as
decimal units such as ``KB`` and ``MB``.

Integer values, or strings without a unit suffix, are interpreted as bytes.

.. seealso::

   `humanize_rs bytes <https://docs.rs/humanize-rs/0.1.5/humanize_rs/bytes/index.html>`_

.. _conf_value_humanize_u32:

humanize u32
============

**yaml value**: int | str

For *str* value, it support units of 2^10 like "KiB", "MiB", or units of 1000 like "KB", "MB".

For *int* value or *str* value without unit, the unit will be bytes.

.. _conf_value_humanize_duration:

humanize duration
=================

**yaml value**: int | str

String values must include at least one unit. Composite values such as
``1h 30m 71s`` are also supported.
See `duration units`_ for the full list of supported units.

Integer and floating-point values are interpreted as seconds.

.. seealso::

   `humanize_rs duration <https://docs.rs/humanize-rs/0.1.5/humanize_rs/duration/index.html>`_

.. _duration units: https://docs.rs/humanize-rs/0.1.5/src/humanize_rs/duration/mod.rs.html#115

.. _conf_value_upstream_str:

upstream str
============

**yaml value**: str

String in ``<ip>[:<port>]`` or ``<domain>[:<port>]`` format.

If the port is omitted, it defaults to ``0``.

.. _conf_value_url_str:

url str
=======

**yaml value**: str

The string must be a valid URL.

.. _conf_value_ascii_str:

ascii str
=========

**yaml value**: str

The string must contain ASCII characters only.

.. _conf_value_regex_str:

regex str
=========

**yaml value**: str

Regular-expression string.

.. _conf_value_rfc3339_datetime_str:

rfc3339 datetime str
====================

**yaml value**: str

The string must be a valid RFC 3339 datetime.

.. _conf_value_selective_pick_policy:

selective pick policy
=====================

**yaml value**: str

Selection policy used for selective vectors.

The following values are supported:

* random

  The default one.

* serial | sequence

  For nodes with the same weights, the order is kept as in the config.

* round_robin | rr

  For nodes with the same weights, the order is kept as in the config.

* ketama

  Ketama Consistent Hash. The key format is defined in the context of each selective vector.

* rendezvous

  Rendezvous Hash. The key format is defined in the context of each selective vector.

* jump_hash

  Jump Consistent Hash. The key format is defined in the context of each selective vector.

.. _conf_value_weighted_upstream_addr:

weighted upstream addr
======================

**yaml value**: map | string

A weighted :ref:`upstream str <conf_value_upstream_str>` value suitable for use
inside a selective vector.

The map consists 2 fields:

* addr

  **required**, **type**: :ref:`upstream str <conf_value_upstream_str>`

  Upstream address.

* weight

  **optional**, **type**: f64

  Weight assigned to the upstream value.
  When used internally, it may be converted to the smallest ``u32`` greater
  than or equal to the ``f64`` value.

  **default**: 1.0

If the value is a string, it is treated as the ``addr`` field and ``weight``
uses the default value.

.. _conf_value_list:

list
====

**yaml value**: mix

A list container for values of type ``T``.

The value can be either a single ``T`` or a sequence of ``T`` values.
