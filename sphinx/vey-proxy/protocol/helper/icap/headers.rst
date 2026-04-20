.. _protocol_helper_icap_headers:

====================
ICAP Request Headers
====================

``vey-icap-client`` always builds the base ICAP request line and standard ICAP
headers such as ``Host``. It may also add the following helper headers to ICAP
requests sent by ``vey-proxy``.

Client Identity Headers
-----------------------

These headers are added when ``vey-proxy`` provides client address or user
information to the ICAP adapter:

- X-Client-IP

  The client IP address.

- X-Client-Port

  The client port.

- X-Client-Username

  The authenticated username, URL-encoded before being written into the ICAP
  request header.

- X-Authenticated-User

  The authenticated username encoded in ICAP-compatible form as
  ``Local://<username>`` and then Base64 encoded.

Shared Response Headers
-----------------------

The following headers are added only for ICAP ``RESPMOD`` requests when
``respond_shared_names`` is configured on the ICAP service:

- Any configured shared header name

  Each configured header is copied from the original HTTP response into the
  ICAP request header if that header is present on the response.

Example:

.. code-block:: yaml

   icap_respmod_service:
     url: icap://127.0.0.1:1344/echo
     respond_shared_names:
       - X-Request-Id
       - X-Trace-Id

Related Pages
-------------

- :doc:`http`
- :doc:`h2`
- :doc:`imap`
- :doc:`smtp`
