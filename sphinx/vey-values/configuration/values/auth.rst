.. _configure_auth_value_types:

****
Auth
****

.. availability::

   - ``vey-proxy``: available
   - ``vey-gateway``: not currently used
   - ``vey-keyless``: not currently used
   - ``vey-statsd``: not currently used

This page documents value types related to authentication and authorization.

.. _conf_value_username:

username
========

**yaml value**: str

UTF-8 username used in the relevant configuration context.
It must be at most 255 bytes.

Only string YAML values are accepted. The loader rejects usernames containing
``:`` because that separator is used by multiple protocols.

.. _conf_value_password:

password
========

**yaml value**: str

UTF-8 password used in the relevant configuration context.
It must be at most 255 bytes.

Only string YAML values are accepted.

.. _conf_value_facts_match_value:

facts_match_value
=================

**yaml value**: str | map

Fact type and fact value used by fact-based authentication.
The value can be either a string in the form
``<fact-type>:<fact-value>`` or a map with a single
``<fact-type>: <fact-value>`` entry.

In string form, the first ``:`` separates the fact type from the value. Any
leading whitespace in the fact value is ignored.

The fact-type should be one of:

- ip

  `<fact-value>` should be :ref:`ip addr str <conf_value_ip_addr_str>`.
  It will match if the auth fact is exactly this IP address.

- net

  `<fact-value>` should be :ref:`ip network str <conf_value_ip_network_str>`.
  It will match if the auth fact is an IP address contained in that CIDR range,
  and it is preferred by longest-prefix match.

- domain | exact-domain

  `<fact-value>` should be :ref:`domain str <conf_value_domain>`.
  It will match if the auth fact is exactly this domain.

- child-domain

  `<fact-value>` should be :ref:`domain str <conf_value_domain>`.
  It will match if the auth fact is a child domain of this domain.

Examples:

.. code-block:: yaml

   match_by_facts:
     - ip: 192.0.2.10
     - net: 192.0.2.0/24
     - child-domain: corp.example.net
     - "domain:api.example.net"

.. availability::


   - ``vey-proxy``: available since ``1.13.0``
