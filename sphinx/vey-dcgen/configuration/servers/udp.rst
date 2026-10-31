.. _configuration_server_udp:

***
udp
***

This server answers certificate requests on one UDP address.

When at least one worker is running, the server binds one reuseport socket on
each worker and ignores ``listen.instance``. Without workers, it binds the
configured instance count on the main runtime. The default instance count is 1.

A datagram larger than 16384 bytes is truncated.

On Linux, a build with the ``ebpf`` feature attaches the UDP reuseport selector
used for a lossless upgrade. The selector is attached when ``use_ebpf`` is
true, or when ``use_ebpf`` is omitted and the process uid is 0. A non-root
process does not attach it unless ``use_ebpf`` is set. If attachment fails and
``fail_on_ebpf_error`` is false, the server logs a warning and continues
without the selector.

.. versionadded:: 0.10.0

The following common keys are supported:

* :ref:`name <conf_server_common_name>`
* :ref:`type <conf_server_common_type>`

listen
======

**required**, **type**: :external+values:ref:`udp listen <conf_value_udp_listen>`

Set the listening socket for this server.

The value may be a port, a socket address string, or the full listen map.
``-G`` names the daemon group and its control socket. It does not select this
address.
