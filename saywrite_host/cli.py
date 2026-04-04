from __future__ import annotations

import argparse

from .server import HostServer
from .oneshot import run_dictation_once


def main() -> int:
    parser = argparse.ArgumentParser(prog="saywrite-host")
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("serve")
    dictate_once = subparsers.add_parser("dictate-once")
    dictate_once.add_argument("--seconds", type=int, default=5)
    args = parser.parse_args()

    if args.command == "serve":
        HostServer().serve_forever()
        return 0
    if args.command == "dictate-once":
        print(run_dictation_once(duration_seconds=max(1, args.seconds)))
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
