@0xb71d4c08e5a2963f;

using Types = import "types.capnp";

interface ProcControl {
  version @0 () -> (version :Text);
  offline @1 () -> (result :Types.OperationResult);
  cancelShutdown @2 () -> (result :Types.OperationResult);
  releaseController @3 () -> (result :Types.OperationResult);

  # Re-read geoip_db from the config file and swap the running databases.
  reload @4 () -> (result :Types.OperationResult);

  query @5 (ip :Text) -> (result :Types.QueryResult);

  # Re-read the configured country database file and swap it in.
  reloadCountry @6 () -> (result :Types.OperationResult);

  # Re-read the configured ASN database file and swap it in.
  reloadAsn @7 () -> (result :Types.OperationResult);
}
