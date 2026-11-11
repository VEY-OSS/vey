@0xe31a7c90b4d26f58;

using Types = import "types.capnp";

interface ProcControl {
  version @0 () -> (version :Text);
  offline @1 () -> (result :Types.OperationResult);
  cancelShutdown @2 () -> (result :Types.OperationResult);
  releaseController @3 () -> (result :Types.OperationResult);

  # Re-read server entries from the config file and apply the difference.
  reload @4 () -> (result :Types.OperationResult);

  # Re-read the issuer section from the main config. Listeners stay up.
  reloadIssuer @5 () -> (result :Types.OperationResult);

  # Re-read one site file. Apply it only when the loaded id matches.
  reloadSite @6 (name :Text) -> (result :Types.OperationResult);
}
