@0xc4e1a90b7d382f15;

struct Error {
  code @0 :Int32 = -1;
  reason @1 :Text;
}

struct OperationResult {
  union {
    ok @0 :Text;
    err @1 :Error;
  }
}

struct LocationRecord {
  network @0 :Text;
  country @1 :Text;
  continent @2 :Text;
  asn @3 :UInt32;
  asnSet @4 :Bool;
  ispName @5 :Text;
  ispDomain @6 :Text;
}

struct QueryResult {
  union {
    location @0 :LocationRecord;
    err @1 :Error;
  }
}
