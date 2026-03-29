.. _configuration_stat:

****
Stat
****

This section describes metrics export configuration. It is optional and cannot
be reloaded. If present, it must be defined in the main configuration file.

The value must be of type :external+values:ref:`statsd client config <conf_value_statsd_client_config>`.
The default metric prefix is ``vey-proxy``.

This section is loaded only during process startup. Changes take effect after a
restart, not a hot reload.
