
.. _configure_resolve_value_types:

*******
Resolve
*******

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: not currently used
   - ``vey-keyless``: not currently used
   - ``vey-statsd``: not currently used

.. _conf_value_resolve_strategy:

Resolve Strategy
================

**yaml value**: mix

``Resolve Strategy`` is not a resolver configuration by itself. It is used by
components that call resolvers.

The value is a map with the following keys:

query
-----

**optional**, **type**: enum str

Query strategy used by the resolver when resolving a name.

The value should be:

* Ipv4Only
* Ipv6Only
* Ipv4First (default)
* Ipv6First

pick
----

**optional**, **type**: enum str

Selection strategy used when choosing the final IP address from all results.

The value should be:

* Random (default)
* First

.. _conf_value_resolve_redirection:

Resolve Redirection
===================

**yaml value**: mix

``Resolve Redirection`` is also used by resolver consumers rather than by the
resolver itself.

The value can be a sequence containing multiple rules of type
:ref:`resolve redirection rule <conf_value_resolve_redirection_rule>`.

It can also be a map. In that form, each key-value pair becomes one rule, where
the key is the ``exact`` value and the value is the ``to`` value.

.. _conf_value_resolve_redirection_rule:

Resolve Redirection Rule
------------------------

Each rule should be a map with the following keys:

* exact

  **optional**, **type**: :ref:`domain <conf_value_domain>` | list

  Exact domain to replace.

  .. availability::


     - ``vey-proxy``: changed in ``1.13.0``: allow list values

* parent

  **optional**, **type**: :ref:`domain <conf_value_domain>` | list

  Parent domain to replace.

  .. availability::


     - ``vey-proxy``: changed in ``1.13.0``: allow list values

* to

  **required**, **type**: mix

  Replacement value for the matched entry.

  For *exact* match, the value should be :ref:`host <conf_value_host>` or an array of ip addresses.

  For *parent* match, the value should be :ref:`domain <conf_value_domain>`.

Each rule must set either ``exact`` or ``parent``.
