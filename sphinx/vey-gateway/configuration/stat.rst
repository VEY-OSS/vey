.. _configuration_stat:

****
Stat
****

This page describes the optional ``stat`` section, which cannot be reloaded.
If present, it must be defined in the main configuration file.

The value must be a
:external+values:ref:`statsd client config <conf_value_statsd_client_config>`.
Unless overridden, the default metrics prefix is ``vey-gateway``.
