
******************
Common Value Types
******************

Many VEY applications reuse the same configuration value types.
This shared reference documents their meaning, accepted syntax, and common
sub-structures in one place.

.. availability::

   - ``vey-proxy``: uses this shared reference from ``1.13.0`` onward
   - ``vey-gateway``: uses this shared reference from ``0.4.0`` onward
   - ``vey-keyless``: uses this shared reference from ``0.5.0`` onward
   - ``vey-statsd``: uses this shared reference from ``0.2.0`` onward

Each value-family page below includes a page-level availability summary. Where
an individual shared value is only available in a subset of applications, or
was added later than the rest of the page, an item-level availability block is
shown next to that value as well.

.. toctree::

   base
   auth
   fs
   network
   acl
   audit
   db
   dpi
   tls
   quic
   rate_limit
   resolve
   metrics
   route
   runtime
   geoip
