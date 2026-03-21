.. _configuration_escaper_comply_audit:

************
comply_audit
************

.. versionadded:: 1.9.9

This escaper overrides the auditor selected by the server side.

There is no path selection support for this escaper.

Config Keys
===========

next
----

**required**, **type**: str

Set the next escaper in the chain.

auditor
-------

**required**, **type**: str

Set the auditor to apply.
