@0xe4b17c2a90d35f81;

using Types = import "types.capnp";

struct UpstreamPeer {
  addr @0 :Text;
  configWeight @1 :Float64;
  weight @2 :Float64;
}

struct ListUpstreamResult {
  union {
    peers @0 :List(UpstreamPeer);
    err @1 :Types.Error;
  }
}

struct UpstreamPeerHealth {
  addr @0 :Text;
  fails @1 :UInt32;
  unavailable @2 :Bool;
  # Milliseconds until this peer can be selected again. Zero when it is available.
  recoverInMs @3 :UInt64;
}

struct ListUpstreamHealthResult {
  union {
    peers @0 :List(UpstreamPeerHealth);
    err @1 :Types.Error;
  }
}

interface SiteGroupControl {
  listUpstream @0 (siteId :Text) -> (result :ListUpstreamResult);
  setUpstreamWeight @1 (siteId :Text, addr :Text, weight :Float64) -> (result :Types.OperationResult);
  listUpstreamHealth @2 (siteId :Text) -> (result :ListUpstreamHealthResult);
}
