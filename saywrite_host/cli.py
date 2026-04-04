from __future__ import annotations

import argparse

from .server import HostServer


def main() -> int:
    parser = argparse.ArgumentParser(prog="saywrite-host")
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("serve")
    args = parser.parse_args()

    if args.command == "serve":
        HostServer().serve_forever()
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
