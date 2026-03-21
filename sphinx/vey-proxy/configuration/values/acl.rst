
.. _configure_acl_value_types:

***
ACL
***

This page documents the ACL-related value types used throughout the
configuration reference.

Basic Type
==========

.. _conf_value_acl_action:

acl action
----------

**yaml value**: str

There are four ACL actions:

* forbid_log

  Forbid if match the rule and log. Alternatives: deny_log, reject_log.

* forbid

  Forbid if match the rule. Alternatives: deny, reject.

* permit_log

  Permit if match the rule and log. Alternatives: allow_log, accept_log.

* permit

  Permit if match the rule. Alternatives: allow, accept.

The match order is the same as the list order above.

.. _conf_value_acl_rule:

acl rule
--------

**yaml value**: mix

All ACL rule types share the same configuration structure described in this
section.

An ACL rule consists of multiple records, each associated with an
:ref:`acl action <conf_value_acl_action>`.
A default action can also be set for the case where no record matches.

When expressed as a map, the value supports the following fields:

* default

  Default ACL action used when no rule matches.

  Default action if rule is set but with *default* omitted: forbid if not specified in the rule's doc.

* any of the acl actions as the key str

  The value should be either a valid record or a list of valid records, and the
  key name determines the ACL action.
  See the detailed types below for the record format.

The value can also be written as a single record or a list of records. In that
form, matching records are permitted without logging.

Unless a detailed type says otherwise, the default unmatched action is
**forbid** and the default matched action is **permit**.

.. _conf_value_acl_rule_set:

acl rule set
------------

**yaml value**: seq

An ACL rule set is an ordered group of at least two ACL rules. The precise
evaluation order depends on the concrete rule-set type described below.

If any record in any rule matches, the corresponding ACL action is used. If no
rule matches, the unmatched actions of all component rules are compared and the
strictest one is used. There is therefore no separate default unmatched action
at the rule-set level.

Detail Type
===========

.. _conf_value_network_acl_rule:

network acl rule
----------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be :ref:`ip network str <conf_value_ip_network_str>`.

.. _conf_value_egress_network_acl_rule:

egress network acl rule
-----------------------

**yaml value**: :ref:`network acl rule <conf_value_network_acl_rule>`

The same type as network acl rule. Default added: forbid unspecified, loopback, link-local and discard-only addresses.

.. _conf_value_ingress_network_acl_rule:

ingress network acl rule
------------------------

**yaml value**: :ref:`network acl rule <conf_value_network_acl_rule>`

The same type as network acl rule. Default added: permit 127.0.0.1 and ::1.

.. _conf_value_dst_subnet_acl_rule:

dst subnet acl rule
-------------------

**yaml value**: :ref:`network acl rule <conf_value_network_acl_rule>`

The same type as network acl rule. Default added: forbid unspecified, loopback and link-local addresses.

.. _conf_value_exact_host_acl_rule:

exact host acl rule
-------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be :ref:`host <conf_value_host>`.

.. _conf_value_exact_port_acl_rule:

exact port acl rule
-------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be :ref:`ports <conf_value_ports>`.

.. _conf_value_child_domain_acl_rule:

child domain acl rule
---------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

Specify the parent domain to match, all children domain in this domain will be matched.

The record type should be :ref:`domain <conf_value_domain>`.

.. _conf_value_regex_domain_acl_rule:

regex domain acl rule
---------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be a map or :ref:`regex str <conf_value_regex_str>`.

The following keys are required for the map format:

 - parent

   **required**, **type**: :ref:`domain <conf_value_domain>`

   Set the parent domain to match.

 - regex

   **required**, **type**: :ref:`regex str <conf_value_regex_str>`

   Set the regex to match the sub part of the domain.

For str format, the regex will match against the full domain.

.. versionadded:: 1.11.5

.. _conf_value_regex_set_acl_rule:

regex set acl rule
------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be :ref:`regex str <conf_value_regex_str>`.

.. _conf_value_dst_host_acl_rule_set:

dst host acl rule set
---------------------

**yaml value**: :ref:`acl rule set <conf_value_acl_rule_set>`

This rule set is used to match dst host for each request.

Consisted of the following rules:

* exact_match

  **optional**, **type**: :ref:`exact host acl rule <conf_value_exact_host_acl_rule>`

* child_match

  **optional**, **type**: :ref:`child domain acl rule <conf_value_child_domain_acl_rule>`

  Match only if the host is a domain.

* regex_match

  **optional**, **type**: :ref:`regex domain acl rule <conf_value_regex_domain_acl_rule>`

  Match only if the host is a domain.

* subnet_match

  **optional**, **type**: :ref:`dst subnet acl rule <conf_value_dst_subnet_acl_rule>`

  Match only if the host is an IP Address.

The match order is the same as the list order above.

.. _conf_value_user_agent_acl_rule:

user agent acl rule
-------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be a valid **product** string as specified in `rfc7231 User-Agent`_.

The default missed action is **permit** and the default found action is **forbid**.

.. _rfc7231 User-Agent: https://tools.ietf.org/html/rfc7231#section-5.5.3

.. _conf_value_proxy_request_acl_rule:

proxy request acl rule
----------------------

**yaml value**: :ref:`acl rule <conf_value_acl_rule>`

The record type should be a valid :ref:`proxy request type <conf_value_proxy_request_type>`.
