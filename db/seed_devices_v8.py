#!/usr/bin/env python3
"""
seed_devices_v8.py — reads config/devices.toml, outputs SQL to seed dim_device
and dim_site stubs for any site_key not already present.

Requires: Python 3.9+ (no external packages). Rocky 9 ships Python 3.9.

Usage (run from repo root):
    python3 db/seed_devices_v8.py | \
      psql "host=localhost port=5432 dbname=alarms user=bht password=bht_dev_pw"

Or with explicit TOML path:
    python3 db/seed_devices_v8.py /opt/bht/config/devices.toml | psql "$DSN"

On Rocky 9 target (already has the toml in /opt/bht/config/):
    python3 seed_devices_v8.py /opt/bht/config/devices.toml | \
      psql "host=localhost dbname=alarms user=bht password=bht_dev_pw"
"""

import re
import sys
import os


def parse_devices(path: str) -> list[dict]:
    """Parse devices.toml without external deps. Handles all field types."""
    text = open(path, encoding="utf-8").read()
    blocks = re.split(r"\[\[device\]\]", text)[1:]  # skip file header

    devices = []
    for block in blocks:
        def s(field: str, default: str = "") -> str:
            """Extract a quoted string field."""
            m = re.search(rf'^\s*{field}\s*=\s*"([^"]*)"', block, re.MULTILINE)
            return m.group(1) if m else default

        def i(field: str, default: int = 0) -> int:
            """Extract an integer field."""
            m = re.search(rf"^\s*{field}\s*=\s*(\d+)", block, re.MULTILINE)
            return int(m.group(1)) if m else default

        def b(field: str, default: bool = False) -> bool:
            """Extract a boolean field."""
            m = re.search(rf"^\s*{field}\s*=\s*(true|false)", block, re.MULTILINE)
            return (m.group(1) == "true") if m else default

        ip = s("ip")
        if not ip:
            continue  # skip malformed blocks

        # 'type' is a Python keyword — match literally
        dev_type_m = re.search(r'^\s*type\s*=\s*"([^"]*)"', block, re.MULTILINE)
        dev_type = dev_type_m.group(1) if dev_type_m else "eaton"

        devices.append({
            "ip":       ip,
            "port":     i("port", 502),
            "unit_id":  i("unit", 1),
            "site_key": s("site_key"),
            "name":     s("name", s("site_key")),
            "dev_type": dev_type,
            "base0":    b("base0", False),
            "fne":      b("fne", False),
            "enabled":  b("enabled", True),
        })

    return devices


def pg_bool(v: bool) -> str:
    return "true" if v else "false"


def pg_str(v: str) -> str:
    """Escape for PostgreSQL single-quoted string literal."""
    return v.replace("'", "''")


def main():
    if len(sys.argv) > 1:
        toml_path = sys.argv[1]
    else:
        # Try locations in order: repo root (dev machine), deploy target (Rocky 9)
        candidates = [
            os.path.join(os.getcwd(), "config", "devices.toml"),
            "/opt/bht/config/devices.toml",
            os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "config", "devices.toml"),
        ]
        toml_path = next((p for p in candidates if os.path.exists(p)), None)
        if toml_path is None:
            print(f"-- ERROR: devices.toml not found. Tried:", file=sys.stderr)
            for p in candidates:
                print(f"--   {p}", file=sys.stderr)
            print("-- Pass the path explicitly: python3 seed_devices_v8.py /path/to/devices.toml", file=sys.stderr)
            sys.exit(1)

    if not os.path.exists(toml_path):
        print(f"-- ERROR: {toml_path} not found", file=sys.stderr)
        sys.exit(1)

    devices = parse_devices(toml_path)
    if not devices:
        print("-- ERROR: no [[device]] blocks found", file=sys.stderr)
        sys.exit(1)

    unique_site_keys = sorted({d["site_key"] for d in devices if d["site_key"]})

    print("BEGIN;")
    print()

    # ----------------------------------------------------------------
    # 1. dim_site stubs for any site_key not already in dim_site
    # ----------------------------------------------------------------
    print("-- Step 1: insert dim_site stub rows for device site_keys not already present")
    print("INSERT INTO dim_site (site_key, display_name, is_stub)")
    print("SELECT v.site_key, v.site_key, true")
    print("FROM (VALUES")
    for idx, sk in enumerate(unique_site_keys):
        comma = "," if idx < len(unique_site_keys) - 1 else ""
        print(f"  ('{pg_str(sk)}'){comma}")
    print(") AS v(site_key)")
    print("WHERE NOT EXISTS (SELECT 1 FROM dim_site s WHERE s.site_key = v.site_key);")
    print()

    # ----------------------------------------------------------------
    # 2. dim_device seed — one row per (ip, unit_id)
    # ----------------------------------------------------------------
    print(f"-- Step 2: seed dim_device ({len(devices)} devices from devices.toml)")
    print("INSERT INTO dim_device")
    print("  (ip, port, unit_id, site_key, dev_type, base0, fne, enabled, name, added_by)")
    print("VALUES")
    for idx, d in enumerate(devices):
        comma = "," if idx < len(devices) - 1 else ""
        print(
            f"  ('{d['ip']}', {d['port']}, {d['unit_id']}, "
            f"'{pg_str(d['site_key'])}', '{d['dev_type']}', "
            f"{pg_bool(d['base0'])}, {pg_bool(d['fne'])}, {pg_bool(d['enabled'])}, "
            f"'{pg_str(d['name'])}', 'toml_import'){comma}"
        )
    print("ON CONFLICT (ip, unit_id) DO NOTHING;")
    print()

    # ----------------------------------------------------------------
    # 3. Summary
    # ----------------------------------------------------------------
    print("-- Summary")
    print("SELECT")
    print("  (SELECT count(*) FROM dim_device)                              AS devices_total,")
    print("  (SELECT count(*) FILTER (WHERE enabled) FROM dim_device)       AS devices_enabled,")
    print("  (SELECT count(*) FILTER (WHERE is_stub) FROM dim_site)         AS stub_sites;")
    print()
    print("COMMIT;")
    print(f"-- seed_devices_v8: {len(devices)} device rows, {len(unique_site_keys)} unique site_keys")


if __name__ == "__main__":
    main()
