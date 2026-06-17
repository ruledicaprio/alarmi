# Deploying to the Rocky 9 LXC (192.168.108.88)

The isolated box has **no toolchain and no Docker** — so we build on your PC and
ship self-contained artifacts. Only TimescaleDB is installed natively on Rocky.

## 1. Database (once)

Copy the repo (or just `db/` + `deploy/`) to the box, then:

```bash
sudo DB_PASS='choose-a-strong-pw' bash deploy/rocky_setup_timescaledb.sh
DB_PASS='choose-a-strong-pw' bash deploy/rocky_apply_schema.sh
```

`rocky_setup_timescaledb.sh` installs PostgreSQL 16 + TimescaleDB from PGDG +
Timescale repos, runs `timescaledb-tune`, creates the `bht` role + `alarms` DB.
`rocky_apply_schema.sh` loads Stage-1 + Stage-2 schema and the 769 seed sites.

### Air-gapped fallback (if `dnf` can't reach repos)

On a connected EL9 box: `dnf download --resolve --alldeps postgresql16-server \
postgresql16-contrib timescaledb-2-postgresql-16` → tar the RPMs → copy over the
tunnel → `sudo dnf install ./*.rpm` on Rocky, then run steps 3–5 of the setup
script manually.

## 2. Rust binaries — build static (musl) via Docker on your PC, copy, run

The Rocky LXC can't run Docker (no Proxmox host root to enable nesting/keyctl) and
has no toolchain. So build on your home PC **inside Docker** — nothing to install
on the host, and the static musl output has no glibc dependency, so it runs on the
LXC as-is. No VM or new container required.

```bash
bash deploy/build_in_docker.sh        # produces target/x86_64-unknown-linux-musl/release/{bht-api,bht-poller}
```
PowerShell one-liner equivalent (Docker Desktop):
```powershell
docker run --rm -v ${PWD}:/src -w /src rust:slim bash -euc "rustup target add x86_64-unknown-linux-musl && apt-get update -qq && apt-get install -y -qq musl-tools && cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller"
```

Ship the artifacts (over your existing tunnel / scp):

```bash
ssh -p 51122 rusmir@msocmsoc.freemyip.com    # then hop to the Rocky box
# from the build host:
scp target/x86_64-unknown-linux-musl/release/bht-poller \
    target/x86_64-unknown-linux-musl/release/bht-api \
    config/*.toml  user@192.168.108.88:/opt/bht/
```

Layout on Rocky:
```
/opt/bht/
  bht-poller            bht-api
  config/poller.toml    config/devices.toml  config/eaton_alarms.toml  config/api.toml
```

## 3. systemd services

```bash
sudo cp deploy/bht-poller.service deploy/bht-api.service /etc/systemd/system/
sudo useradd -r -s /sbin/nologin bht 2>/dev/null || true
sudo chown -R bht:bht /opt/bht
sudo systemctl daemon-reload
sudo systemctl enable --now bht-poller bht-api
journalctl -u bht-poller -f      # watch a live poll cycle
journalctl -u bht-api -f
```

## 4. Verify the whole chain on the box

```bash
curl -s localhost:8080/api/health
curl -s 'localhost:8080/api/alarms/active' | head
psql -U bht -d alarms -c "SELECT source, count(*) FROM fact_event GROUP BY 1;"
```

The poller reaches the 10.10.x Modbus VLAN (confirmed: `ping 10.10.1.17`), writes
events + measurements; the API serves them to the dashboard. SELinux note: if
services fail to bind/connect, check `ausearch -m avc -ts recent` — Rocky ships
enforcing; a port label or `semanage` rule may be needed for non-standard ports.
