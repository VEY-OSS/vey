.. _configuration_server_unix:

****
unix
****

This server answers IP location queries on one Unix datagram socket.

The listener always runs on the main runtime. It is available only on Unix.
On other platforms a server of this type is rejected when the config is loaded.

.. versionadded:: 0.5.0

The following common keys are supported:

* :ref:`name <conf_server_common_name>`
* :ref:`type <conf_server_common_type>`

listen
======

**required**, **type**: :external+values:ref:`absolute path <conf_value_absolute_path>`

Set the datagram socket path. A relative path is rejected.

The path is the socket address this process binds. It does not have to exist
before startup. The parent directory must exist.
