.. _protocol_client:

###############
Client Protocol
###############

``vey-proxy`` accepts the following client-side protocols:

* HTTP proxy

  - Supports HTTP forward, HTTP CONNECT, FTP over HTTP and HTTP CONNECT-UDP.
  - HTTPS forward is also supported, but is disabled by default.
  - `easy-proxy` Well-Known URI is also supported.
  - HTTP/1.0 and HTTP/1.1 are supported. HTTP/2 and HTTP/3 are not currently supported on the client side.
  - Only Basic authentication is supported.
  - TLS 1.2 and later can be enabled.
  - See :doc:`http_custom_headers` for custom headers.
  - See :doc:`http_custom_codes` for custom response codes.
  - See :doc:`egress_path_selection` for request-driven and user-driven egress path selection.

* SOCKS proxy

  - SOCKS4 and SOCKS4a are supported by most escapers. Ident verification is not supported.
  - SOCKS5 TCP CONNECT is supported by most escapers.
  - SOCKS5 UDP ASSOCIATE is supported by some escapers, but is disabled by default on the server side. The default UDP
    mode is UDP CONNECT, which is simpler but requires the target address to remain the same for all packets.
    If no explicit bind IP is configured, the client-side TCP and UDP connections should use the same address family.
  - SOCKS5 username/password authentication is the only SOCKS authentication method currently supported.
  - TLS and DTLS are not yet supported.
  - SOCKS6 draft is not supported.

.. toctree::
   :hidden:

   http_custom_headers
   http_custom_codes
   egress_path_selection
