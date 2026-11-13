#!/usr/bin/env python3
"""Print llvm-cov --object arguments from cargo --message-format=json.

Cargo 1.100 stopped writing test binaries to target/debug/deps. The
compiler-artifact "executable" field is the supported way to find them.
"""

import json
import sys


def main() -> int:
    exclude = set()
    args = sys.argv[1:]
    index = 0
    while index < len(args):
        if args[index] == "-object" and index + 1 < len(args):
            exclude.add(args[index + 1])
            index += 2
        else:
            exclude.add(args[index])
            index += 1

    seen = []
    for line in sys.stdin:
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-artifact":
            continue
        kinds = msg.get("target", {}).get("kind") or []
        if "custom-build" in kinds:
            continue
        exe = msg.get("executable")
        if not isinstance(exe, str) or not exe or exe.endswith(".so"):
            continue
        if exe in exclude or exe in seen:
            continue
        seen.append(exe)

    if not seen:
        sys.stderr.write("coverage: cargo reported no test executables\n")
        return 1
    sys.stdout.write(" ".join(f"-object {path}" for path in seen))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
