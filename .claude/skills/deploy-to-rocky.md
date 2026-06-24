# Deploy to Rocky 9 — bht-alarm

Target: Rocky Linux 9 LXC, IP `192.168.108.88`, install dir `/opt/bht`, user `bht`.

---

## Pre-flight

```bash
ls -lh ~/bht-upgrade.tar.gz
sha256sum ~/bht-upgrade.tar.gz
```

---

## Step 1 — Transfer (Python HTTP server — the only method)

No `scp`. Transfer by serving the tarball over HTTP from the work PC and curling on Rocky.

**On work PC (WSL terminal):**
```bash
cd ~
python3 -m http.server 8000
# note the LAN IP shown, or check: hostname -I
```

**On Rocky:**
```bash
curl -O http://<work-PC-LAN-IP>:8000/bht-upgrade.tar.gz
# example: curl -O http://192.168.82.205:8000/bht-upgrade.tar.gz
```

Stop the HTTP server (`Ctrl+C`) after Rocky confirms the download.

---

## Step 2 — Deploy bht-api + frontend (automated)

`rocky_deploy.sh` handles **bht-api + web/dist only**. For bht-poller or neteco-poller, use the manual steps below.

```bash
ssh root@192.168.108.88 'bash ~/rocky_deploy.sh bht-upgrade.tar.gz'
```

---

## Step 3 — Deploy bht-poller / neteco-poller (manual)

Rocky 9 has **no `tar`** — use python3 to extract.

```bash
ssh root@192.168.108.88
```

On Rocky:
```bash
# Extract tarball (Rocky has no tar)
cd ~
python3 -c "import tarfile; tarfile.open('bht-upgrade.tar.gz').extractall('.')"

# Stop services
sudo systemctl stop bht-poller neteco-poller

# Deploy bht-poller
sudo cp ~/bht-poller /opt/bht/bht-poller
sudo chmod +x /opt/bht/bht-poller
sudo chown bht:bht /opt/bht/bht-poller

# Deploy neteco-poller
sudo cp ~/neteco-poller /opt/bht/neteco-poller
sudo chmod +x /opt/bht/neteco-poller
sudo chown bht:bht /opt/bht/neteco-poller

# Start services
sudo systemctl start bht-poller neteco-poller
sleep 2
sudo systemctl is-active bht-poller neteco-poller
```

---

## Step 4 — Health checks

```bash
curl -sf http://localhost:8080/api/health && echo "bht-api OK"
sudo journalctl -u bht-poller -n 20 --no-pager
sudo journalctl -u neteco-poller -n 20 --no-pager
sudo -u bht psql -d alarms -c "SELECT count(*) FROM fact_event;"
sudo -u bht psql -d alarms -c "SELECT count(*) FROM neteco.alarms WHERE status = 1;"
```

---

## Config files (NOT in tarball)

Config lives at `/opt/bht/config/` on Rocky and is never deployed via tarball:
- `api.toml`, `poller.toml`, `devices.toml`
- `eaton_alarms.toml`, `datakom_alarms.toml`, `smartlogger_alarms.toml`
- `neteco.toml`
- `.neteco.env` — **credentials, never leave Rocky**

To rotate NetEco credentials:
```bash
sudo nano /opt/bht/config/.neteco.env
sudo systemctl restart neteco-poller
```

---

## Staging environment

Same steps, substitute:
- Install dir: `/opt/bht-staging`
- DB: `alarms_staging`
- Service names: `bht-api-staging`, `bht-poller-staging`
