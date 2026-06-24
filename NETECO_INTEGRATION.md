# NetEco NBI Integration

Huawei iSitePower NBI REST API integration with the BHT alarm pipeline.

---

## Architecture

```
NetEco iSitePower (10.10.0.X:31943)
        │
        │  HTTPS REST (pull, every 2 min / 5 min / 6 h)
        ▼
  neteco-poller  ──────────────────────────────────────────────►  neteco.* tables
        │                                                         (PostgreSQL 16)
        │  push callbacks (POST /ingest/neteco/push)                   │
        ◄──────────  nginx :31943 ──► bht-api :8080               bht-api :8080
                                                                       │
                                                                       ▼
                                                              /api/neteco/alarms
                                                         (served to the SPA frontend)
```

**Data flows:**
| Flow | Direction | Interval | Destination |
|------|-----------|----------|-------------|
| Alarm poll (getAlarmList) | neteco-poller → NetEco | 2 min | `neteco.alarms` |
| Metric poll (getDevRealKpi) | neteco-poller → NetEco | 5 min | `neteco.*_metrics` tables |
| Topology sync (getStationList + getDevList) | neteco-poller → NetEco | 6 h | `neteco.sites`, `neteco.devices` |
| Push notifications | NetEco → bht-api | event-driven | `neteco.alarms` |

---

## API Endpoints

### Query

| Method | Path | Query params | Returns |
|--------|------|-------------|---------|
| GET | `/api/neteco/alarms` | `station`, `severity` (1-4), `status` (1/2/4/5/6), `limit`, `offset` | `{total, count, items[]}` |
| GET | `/api/neteco/alarms/summary` | — | `{active, critical, major, minor_warn, affected_stations}` |

### Ingest (push)

| Method | Path | Body | Returns |
|--------|------|------|---------|
| POST | `/ingest/neteco/push` | NetEco alarm push envelope (JSON) | `{accepted, upserted}` |

**Push body format** (same as `getAlarmList` response):
```json
{
  "failCode": 0,
  "data": [
    {
      "alarmId": "string",
      "stationCode": "string",
      "stationName": "string",
      "devId": 123456,
      "devName": "string",
      "devTypeId": 60016,
      "alarmName": "string",
      "alarmCause": "string",
      "alarmType": 2,
      "lev": 1,
      "status": 1,
      "raiseTime": 1719234567000,
      "repairTime": null
    }
  ]
}
```

---

## Authentication

NetEco NBI uses a session token scheme:
- **Login**: `POST /thirdData/login` with `{userName, systemCode}`
- **Token**: returned in `XSRF-TOKEN` response header (valid 30 min)
- **All calls**: include `XSRF-TOKEN: <token>` header
- **Refresh**: neteco-poller refreshes at the 25-min mark (lazy, on next API call)
- **Rate limit**: 5 login calls / 10 min per user — the poller never logs in concurrently

**Config location on Rocky:** `/opt/bht/config/neteco.toml` + `/opt/bht/config/.neteco.env`

The `.neteco.env` file contains `NETECO_USER` and `NETECO_PASSWORD`. It is loaded by
the systemd `EnvironmentFile=` directive and **must never be included in deployment tarballs**.
To rotate credentials: edit `.neteco.env` on Rocky, then `systemctl restart neteco-poller`.

---

## Database Schema

All NetEco data lives in the `neteco` schema (separate from the main `public` schema).

### Key tables

| Table | Purpose | Retention |
|-------|---------|-----------|
| `neteco.sites` | Station registry (station_code → name) | Permanent |
| `neteco.devices` | Device registry per station | Permanent |
| `neteco.alarms` | Active + historical alarms | Permanent (no TTL) |
| `neteco.site_unit_metrics` | Indoor temp, power (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.battery_group_metrics` | SOC, voltage, backup time (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.power_system_metrics` | DC voltage, load current (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.mains_metrics` | AC voltage, phases, frequency (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.ac_input_metrics` | AC input per rectifier group (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.dpdu_metrics` | DC distribution (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.genset_metrics` | Generator metrics (hypertable) | 7-day chunks, compressed after 14d |
| `neteco.site_unit_5m` | 5-min rollup of site_unit_metrics (cagg) | Auto-updated |
| `neteco.battery_5m` | 5-min rollup of battery SOC (cagg) | Auto-updated |

### Alarm severity mapping

| DB value | NetEco `lev` | Label |
|----------|-------------|-------|
| 1 | 1 | Critical |
| 2 | 2 | Major |
| 3 | 3 | Minor |
| 4 | 4 | Warning |

### Alarm status mapping

| DB value | Label | Meaning |
|----------|-------|---------|
| 1 | Active | Unacknowledged active alarm |
| 2 | Acknowledged | Operator has acknowledged |
| 4 | Handled | Under repair |
| 5 | User-clear | Manually cleared by operator |
| 6 | Auto-clear | Cleared automatically by NetEco |

---

## Network Topology (Rocky 9)

```
192.168.108.88 (Rocky LXC)
  ├── bht-api.service          → HTTP 0.0.0.0:8080
  ├── neteco-poller.service    → outbound HTTPS to 10.10.0.X:26335 (or :31943)
  └── nginx (bht-neteco-proxy) → HTTPS 0.0.0.0:443 → proxy to :8080
                                  (for NetEco push callbacks)
```

**Port mapping — iMaster NetEco V600R023C00** (from official port matrix):

| Port | Direction | Description |
|------|-----------|-------------|
| **26335** | Third-party → NetEco | **NBI REST API gateway (APIMLBService)**. This is where `/thirdData/` calls should go. Auth: SSO/token, TLS 1.2 |
| **31943** | Web browser → NetEco | NetEco web UI client login (HTTPS). May also route NBI in some deployments. Auth: username/password, TLS 1.2 |
| **443** | NetEco → our system | Our HTTPS push callback listener. nginx terminates TLS, forwards to bht-api:8080 |

**Port 31943 is the NetEco server's own web UI port — it is not a port we listen on.**
The nginx proxy runs on our standard HTTPS port 443. See `deploy/setup_neteco_proxy.sh`.

**Push callback URL to register in NetEco admin:**
```
https://192.168.108.88/ingest/neteco/push
```

**Troubleshooting 404 on `/thirdData/` calls:**
1. Try port 26335 if 31943 gives 404 — update `url` in `config/neteco.toml`
2. Verify the NBI third-party user is created in NetEco admin (System → Security → User Management → add user with NBI/API role)
3. Verify the NBI service module is enabled on the NetEco server
4. Some deployments prefix the path — try `https://<ip>:26335/service/thirdData/login` if the bare `/thirdData/login` gives 404
5. Run `neteco-poller --fingerprint` to confirm auth flow and see exact error messages

---

## Failure Recovery

| Failure mode | Recovery |
|-------------|----------|
| NetEco auth token expired | Poller auto-invalidates on 401, re-logs on next call |
| NetEco rate limit hit (407) | Poller logs warning; retries on next poll cycle |
| Network timeout | Poller logs error, continues on next cycle; no alarm data gap |
| Push endpoint down | Alarm data still arrives via pull (2-min poll) |
| neteco-poller crash | systemd auto-restart; initial topology sync on startup |
| DB connection lost | tokio-postgres driver surfaces error; poller retries on next tick |
| Compressed chunk query | Transparent via TimescaleDB decompression on read |

---

## Operational Runbook

### Check poller health
```bash
# Tail live logs
sudo journalctl -u neteco-poller -f --no-pager

# One-shot connectivity test (discover devTypeId + KPI keys)
sudo -u bht /opt/bht/neteco-poller --config /opt/bht/config/neteco.toml --once --fingerprint

# Active alarm count
sudo -u bht psql -d alarms -c "SELECT count(*) FROM neteco.alarms WHERE status = 1;"

# Most recent alarm poll timestamp
sudo -u bht psql -d alarms -c "SELECT MAX(last_seen) FROM neteco.alarms;"
```

### Rotate NetEco credentials
```bash
# Edit credentials on Rocky — never copy this file off the machine
sudo nano /opt/bht/config/.neteco.env
# (change NETECO_USER= and NETECO_PASSWORD=)
sudo systemctl restart neteco-poller
sudo journalctl -u neteco-poller -n 20 --no-pager
```

### Verify push proxy
```bash
# On Rocky
curl -sk https://127.0.0.1/api/health && echo "proxy OK"

# Test push endpoint manually
curl -sk -X POST https://127.0.0.1/ingest/neteco/push \
  -H 'Content-Type: application/json' \
  -d '{"failCode":0,"data":[]}' | python3 -m json.tool
```

### Clean up vi swap file
```bash
rm -f /opt/bht/config/.neteco.env.swp /opt/bht/config/.neteco.env.swo
```

### Apply schema (first-time or after migration)
```bash
sudo -u bht psql -d alarms -f /tmp/migrate_neteco_v1.sql
# Verify
sudo -u bht psql -d alarms -c "\dt neteco.*"
```
