.. _log_resolve:

***********
Resolve Log
***********

Resolve logs record resolver-side errors only.

Shared Keys
===========

resolver_type
-------------

**required**, **type**: enum string

  The resolver type.

resolver_name
-------------

**required**, **type**: string

  The resolver name.

Values:

* c-ares
* fail-over
* deny-all

query_type
----------

**required**, **type**: enum string

The query type.

Values:

* A
* AAAA

duration
--------

**required**, **type**: time duration string

The time spent on this query action.

rr_source
---------

**required**, **type**: enum string

The source of the result.

Values:

* cache

  The result is fetched from cache.

* query

  The result came from a real query sent to a remote DNS server.

error_type
----------

**required**, **type**: enum string

The primary error type.

See the definition of **ResolverError** in *lib/vey-resolver/src/error.rs*.

error_subtype
-------------

**required**, **type**: enum string

The secondary error type.

Its meaning depends on the value of **error_type**.

See the definition of **ResolverError** in *lib/vey-resolver/src/error.rs*.

domain
------

**required**, **type**: domain string

The queried domain.

Sub Types
=========

.. toctree::
   :maxdepth: 1

   c_ares
   hickory
   fail_over
   deny_all
