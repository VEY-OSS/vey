.. _configuration_escaper_dummy_deny:

**********
dummy_deny
**********

This escaper rejects every request immediately.

There is no path selection support for this escaper.

The config loader only requires the escaper name. All requests routed here are
rejected immediately.

Config Keys
===========

The following common keys are supported:

* :ref:`extra_metrics_tags <conf_escaper_common_extra_metrics_tags>`
