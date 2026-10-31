@0xb7c2e14a9d05f683;

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
