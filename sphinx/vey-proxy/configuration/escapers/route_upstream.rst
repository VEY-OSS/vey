.. _configuration_escaper_route_upstream:

route_upstream
==============

This escaper selects the next escaper based on rules applied to the upstream address.

There is no path selection support for this escaper.

The following common keys are supported:

* :ref:`default_next <conf_escaper_common_default_next>`

exact_match
-----------

**optional**, **type**: seq | map

If the host part of the upstream address exactly matches a configured rule, the corresponding escaper is selected.

For seq format:

  Each rule is in *map* format, with two keys:

  * next

    **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

    Set the next escaper.

  * hosts

    **optional**, **type**: seq, **alias**: host

    Each element should be :ref:`host <conf_value_host>`.

    A host must not appear in rules for different next escapers.

  Example:

  .. code-block:: yaml

    - next: deny
      hosts:
        - example.net
    - next: allow
      hosts:
        - 192.168.1.1

For map format:

  Each key is the next escaper name, and each value has the same format as ``hosts`` in the sequence form.

  Example:

  .. code-block:: yaml

    deny:
      - example.net
    allow:
      - 192.168.1.1

.. versionchanged:: 1.11.5 support map format

subnet_match
------------

**optional**, **type**: seq | map

If the host is an IP address and matches multiple subnets, the longest-prefix match is used.

For seq format:

  Each rule is in *map* format, with two keys:

  * next

    **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

    Set the next escaper.

  * subnets

    **optional**, **type**: seq, **alias**: subnet

    Each element should be :ref:`ip network str <conf_value_ip_network_str>`.

    A subnet must not appear in rules for different next escapers.

  Example:

  .. code-block:: yaml

    - next: deny
      subnets:
        - 192.168.0.0/16
    - next: allow
      subnets:
        - 192.168.0.0/24

For map format:

  Each key is the next escaper name, and each value has the same format as ``subnets`` in the sequence form.

  Example:

  .. code-block:: yaml

    deny:
      - 192.168.0.0/16
    allow:
      - 192.168.0.0/24

.. versionchanged:: 1.11.5 support map format

child_match
-----------

**optional**, **type**: seq | map

If the upstream domain is a child of a configured domain, the corresponding escaper is selected.

For seq format:

  Each rule is in *map* format, with two keys:

  * next

    **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

    Set the next escaper.

  * domains

    **optional**, **type**: seq, **alias**: domain

    Each element should be :ref:`domain <conf_value_domain>`.

    A domain must not appear in rules for different next escapers.

  Example:

  .. code-block:: yaml

    - next: deny
      domains:
        - example.net
    - next: allow
      domains:
        - test.example.net

For map format:

  Each key is the next escaper name, and each value has the same format as ``domains`` in the sequence form.

  Example:

  .. code-block:: yaml

    deny:
      - example.net
    allow:
      - test.example.net

.. versionchanged:: 1.11.5 support map format

suffix_match
------------

**optional**, **type**: seq | map, **alias**: radix_match

If the upstream domain matches one of the configured suffixes, the corresponding escaper is selected.

For seq format:

  Each rule is in *map* format, with two keys:

  * next

    **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

    Set the next escaper.

  * suffixes

    **optional**, **type**: seq, **alias**: suffix

    Each element should be :ref:`domain <conf_value_domain>`.

    A suffix must not appear in rules for different next escapers.

  Example:

  .. code-block:: yaml

    - next: deny
      suffixes:
        - example.net
    - next: allow
      suffixes:
        - t.example.net
    # test.example.net will match `allow`

For map format:

  Each key is the next escaper name, and each value has the same format as ``suffixes`` in the sequence form.

  .. code-block:: yaml

    deny:
      - example.net
    allow:
      - t.example.net
    # test.example.net will match `allow`

.. versionchanged:: 1.11.5 support map format

regex_match
-----------

**optional**, **type**: seq | map

If the upstream domain matches one of the configured regular expressions, the corresponding escaper is selected.

For seq format:

  Each rule is in *map* format, with two keys:

  * next

    **required**, **type**: :ref:`metric node name <conf_value_metric_node_name>`

    Set the next escaper.

  * rules

    **optional**, **type**: seq, **alias**: rule

    Each element should be a map or :ref:`regex str <conf_value_regex_str>`.

    The following keys are used in the map format:

      - parent

        **optional**, **type**: :ref:`domain <conf_value_domain>`

        Parent domain to strip off, including the trailing ``.``, before applying the regex.
        If omitted, the full domain is matched.

      - regex

        **required**, **type**: :ref:`regex str <conf_value_regex_str>`

        Regular expression to apply.

    A rule must not appear in rules for different next escapers.

  Example:

  .. code-block:: yaml

    - next: deny
      rules:
        - parent: example.net
          regex: abc.*  # only match the sub part
    - next: allow
      rules:
        - parent: example.net
          regex: tes.+ # only match the sub part
        - .*[.]example[.]org  # match the full domain
    # test.example.net will match `allow`

For map format:

  Each key is the next escaper name, and each value has the same format as ``rules`` in the sequence form.

  Example:

  .. code-block:: yaml

    deny:
      - parent: example.net
        regex: abc.*  # only match the sub part
    allow:
      - parent: example.net
        regex: tes.+ # only match the sub part
      - .*[.]example[.]org  # match the full domain
    # test.example.net will match `allow`

.. versionadded:: 1.11.5
