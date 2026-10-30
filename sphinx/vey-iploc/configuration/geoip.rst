.. _configuration_geoip:

*****
GeoIP
*****

.. versionadded:: 0.5.0

The optional ``geoip_db`` section names the database files used by lookups.
It can be reloaded. See :ref:`configuration_reload`.

Country and ASN are independent. A lookup fills only the fields covered by a
matching record. An address that matches neither table produces no response.

Both files use the VEY native CSV format produced by the ``scripts/geoip-dump``
converters. Deb and rpm packages install those scripts in
``/usr/share/vey-iploc/``. Empty lines and lines that start with ``#`` are
skipped. A ``.gz``
or ``.GZ`` file is decompressed before it is parsed. A prefix with host bits
set is truncated to the network address.

.. _conf_geoip_country:

country
=======

**optional**, **type**: :external+values:ref:`file path <conf_value_file_path>`

Path of the country database. A relative path is resolved against the
configuration directory. The file must already exist.

Each data line is ``network,country``. ``network`` is an IP prefix such as
``192.168.1.0/24``. ``country`` is an ISO 3166 Alpha-2 or Alpha-3 country code.
The continent stored with the record is derived from that country code.

**default**: not set, which leaves the running country table unchanged on
reload and empty at startup

.. _conf_geoip_asn:

asn
===

**optional**, **type**: :external+values:ref:`file path <conf_value_file_path>`

Path of the ASN database. A relative path is resolved against the configuration
directory. The file must already exist.

Each data line is ``network,asn``. ``network`` is an IP prefix. ``asn`` is a
decimal autonomous system number. The CSV loader does not set an ISP name or
domain.

**default**: not set, which leaves the running ASN table unchanged on reload
and empty at startup

Example::

   geoip_db:
     country: vey-country.csv.gz
     asn: vey-asn.csv.gz
