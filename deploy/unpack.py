#!/usr/bin/env python3
import sys, tarfile, shutil, subprocess, os

MODE = sys.argv[2] if len(sys.argv) > 2 else "binary"
TARBALL = sys.argv[1]
DEST = "/opt/alarmi"

with tarfile.open(TARBALL) as tar:
    tar.extractall(path="/tmp/alarmi-update")

if MODE == "frontend":
    shutil.rmtree(f"{DEST}/web", ignore_errors=True)
    shutil.move("/tmp/alarmi-update/dist", f"{DEST}/web")
    print("Frontend updated.")

elif MODE == "binary":
    subprocess.run(["systemctl", "stop", "alarmi-server"])
    shutil.copy("/tmp/alarmi-update/alarmi-server", f"{DEST}/bin/alarmi-server")
    os.chmod(f"{DEST}/bin/alarmi-server", 0o755)
    subprocess.run(["systemctl", "start", "alarmi-server"])
    subprocess.run(["systemctl", "status", "alarmi-server"])

elif MODE == "schema":
    migrations = sorted(f for f in os.listdir("/tmp/alarmi-update") if f.endswith(".sql"))
    for m in migrations:
        path = os.path.join("/tmp/alarmi-update", m)
        subprocess.run(["psql", "-U", "alarmi", "-d", "alarmi_db", "-v", "ON_ERROR_STOP=1", "-f", path], check=True)
    print("Schema migrations applied.")
