
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

**yaml value**: str | map

``Resolve Strategy`` is not a resolver configuration by itself. It is used by
components that call resolvers.

The value is a map with the following keys:

The shorthand string form sets only ``query`` and leaves ``pick`` at its
default.

query
-----

**optional**, **type**: enum str

Query strategy used by the resolver when resolving a name.

Matching is case-insensitive and separators such as ``_`` are normalized.
Supported values:

* Ipv4Only
* Ipv6Only
* Ipv4First (default)
* Ipv6First

pick
----

**optional**, **type**: enum str

Selection strategy used when choosing the final IP address from all results.

Matching is case-insensitive. Supported values:

* Random (default)
* First

Example:

.. code-block:: yaml

   resolve_strategy:
     query: ipv6_first
     pick: first

.. _conf_value_resolve_redirection:

Resolve Redirection
===================

**yaml value**: seq | map

``Resolve Redirection`` is also used by resolver consumers rather than by the
resolver itself.

The value can be a sequence containing multiple rules of type
:ref:`resolve redirection rule <conf_value_resolve_redirection_rule>`.

It can also be a map. In that form, each key-value pair becomes one rule, where
the key is the ``exact`` value and the value is the ``to`` value.

Parsing behavior differs by form:

* map form defines only ``exact`` matches
* sequence form supports both ``exact`` and ``parent`` rules
* ``to`` may be a host/domain alias or a list of literal IP addresses

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

  **required**, **type**: :ref:`host <conf_value_host>` | list

  Replacement value for the matched entry.

  .. availability::


      - ``vey-proxy``: changed in ``1.13.1``: allow ip address value for parent domain match

Each rule must set either ``exact`` or ``parent``.

Examples:

.. code-block:: yaml

   resolve_redirection:
     example.net: 192.0.2.10
     example.org:
       - 192.0.2.20
       - 192.0.2.21

.. code-block:: yaml

   resolve_redirection:
     - parent: corp.example.net
       to: edge.example.net
     - exact:
         - api.example.net
         - api2.example.net
       to:
         - 192.0.2.30
         - 192.0.2.31
