.. _configuration_escaper_comply_audit:

************
comply_audit
************

.. versionadded:: 1.9.9

This escaper swaps the active auditor for the current request.

It updates the per-request audit target and then forwards the request to
``next``.

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

Example
-------

.. code-block:: yaml

   next: direct
   auditor: strict-audit
