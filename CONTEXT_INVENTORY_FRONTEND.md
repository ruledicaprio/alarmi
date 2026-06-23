# Frontend Inventory Management — Session Context

## What was built (v8 backend — fully deployed on Rocky 9)

### DB schema additions (migrate_v8.sql — applied)
- `dim_device` table — 301 rows, one per Modbus endpoint (ip, port, unit_id, site_key, dev_type, base0, fne, enabled, name, notes, first_seen, last_polled, last_ok, fail_streak, added_by)
- `UNIQUE INDEX ux_device_ip_unit ON dim_device (ip, unit_id)` — same IP can have multiple Modbus unit_ids (SmartLogger pattern)
- `dim_site.is_stub BOOLEAN` — auto-set when a new site_key appears in ingested events with no prior dim_site row
- `v_device_health` view — classified health per device: ok/degraded/dead/stale/never
- `v_device_orphans` view — IPs seen in fact_event with no dim_device row

### Health classification (v_device_health)
```
never    → last_ok IS NULL (never had a successful poll)
dead     → fail_streak >= 3
degraded → fail_streak > 0
stale    → last_ok < now() - 10 minutes
ok       → otherwise
```

### Poller changes (deployed)
- Loads devices from dim_device WHERE enabled=true (not devices.toml anymore)
- Writes last_polled, last_ok, fail_streak back every cycle
- Hot-reloads device list every 10 cycles (~20 min) — new devices added via API get picked up without restart
- Current cycle: ok=259 fail=42 (42 devices unreachable in 10.10.6.x subnet)

### 8 new API endpoints (bht-api v8 — deployed)

```
GET  /api/inventory/devices
  Query params: region, health (ok|degraded|dead|stale|never), dev_type, enabled, fne, q (search), page, page_size
  Response: { summary: {total,ok,degraded,dead,stale,never,disabled}, total, count, items: [...] }
  Item fields: id, ip, port, unit_id, site_key, site_name, region, dev_type, fne, enabled,
               name, fail_streak, last_polled, last_ok, health, added_by, updated_at

POST /api/inventory/devices
  Body: { ip, site_key, port?, unit_id?, dev_type?, base0?, fne?, enabled?, name?, notes? }
  Upserts by (ip, unit_id). Auto-stubs dim_site if site_key is new.
  Response: { id }

PATCH /api/inventory/devices/:id
  Body: any subset of { site_key, port, unit_id, dev_type, base0, fne, enabled, name, notes }
  COALESCE partial update — omitted fields unchanged.

DELETE /api/inventory/devices/:id
  404 if not found, 200 { deleted: id } on success.

GET  /api/inventory/device-orphans
  IPs in fact_event with no dim_device row.
  Response: { count, items: [{ip, site_key, event_count, last_seen, source}] }

POST /api/inventory/device-orphans/claim
  Body: { ip, site_key, port?, unit_id?, dev_type?, name? }
  Validates IP has event history, inserts into dim_device, stubs dim_site.

GET  /api/inventory/stubs
  dim_site rows where is_stub=true, ordered by event_count DESC.
  Response: { total, count, items: [{site_key, display_name, event_count, device_count, first_seen, updated_at}] }
  Query params: page, page_size

PATCH /api/sites/:site_key
  Body: any subset of { display_name, region, municipality, lat, lon,
                        technologies, has_genset, has_battery, has_solar, is_important, notes }
  Clears is_stub automatically when region is provided.
```

---

## Frontend — current state

**Stack:** Vite + TypeScript + React + Ant Design 5 + ProComponents (@ant-design/pro-components) + React Router 6

**Existing `web/src/pages/Inventory.tsx`** has 3 tabs:
- "Orphan events" → GET /api/inventory/orphans (site_key-level orphans, pre-v8)
- "Silent sites" → GET /api/inventory/stale
- "Region coverage" → GET /api/inventory/coverage

**`web/src/App.tsx`** — `/inventory` route already wired to `<Inventory />`.

**Helper imports available:**
- `import { api, qs } from '../api'` — api() does fetch+json, qs() builds query string
- `import { formatTs } from '../utils'` — formats ISO timestamp
- ProTable, ProCard, Tabs, Tag, Select, Input, Button, Modal, Form from antd / pro-components

---

## Task: Frontend Inventory Management

Extend `web/src/pages/Inventory.tsx` with new tabs using the v8 endpoints.

### Tab: "Device fleet" (priority 1)
- Summary cards at top: total / ok / degraded / dead / stale / never / disabled (from summary object)
- Filter bar: region dropdown, health dropdown (all/ok/degraded/dead/stale/never), dev_type, search input
- ProTable columns: IP, site_key (link to /sites/:key), region, type, health (colored Tag), last_ok (formatTs), fail_streak, enabled toggle, name, actions (edit, delete)
- Health tag colors: ok=green, degraded=orange, dead=red, stale=gray, never=default

### Tab: "Device orphans" (priority 2)
- Table: ip, site_key, event_count, last_seen, source
- "Claim" button per row → modal to fill site_key, dev_type, name → POST /api/inventory/device-orphans/claim

### Tab: "Stub sites" (priority 3)
- Table: site_key, event_count, device_count, first_seen
- "Enrich" button per row → drawer/modal with form fields (display_name, region, municipality, lat, lon, technologies checkboxes, has_genset/battery/solar toggles) → PATCH /api/sites/:site_key

### Add device button (floating or in Device fleet tab header)
- Modal form → POST /api/inventory/devices
- Fields: IP (required), site_key (required), port (default 502), unit_id (default 1), dev_type (select: eaton/smartlogger), name, fne checkbox

---

## Build & Deploy reference

```bash
# WSL — build + pack
cd ~/alarmi-repo/web
docker run --rm -v "$PWD":/web -w /web node:20-slim sh -c "npm run build"
cd ~/alarmi-repo
tar czf ~/bht-upgrade-inv3.tar.gz \
  -C target/x86_64-unknown-linux-musl/release bht-api \
  -C ~/alarmi-repo/web dist \
  -C ~/alarmi-repo deploy/rocky_deploy.sh

cd ~ && python3 -m http.server 8000

# Rocky 9
curl -O http://192.168.82.205:8000/bht-upgrade-inv3.tar.gz
python3 -c "import tarfile,warnings; warnings.filterwarnings('ignore'); tarfile.open('bht-upgrade-inv3.tar.gz').extractall()"
bash ~/deploy/rocky_deploy.sh bht-upgrade-inv3.tar.gz
```
Note: only web changed for frontend tasks — no need to rebuild Rust. Pack only bht-api (already deployed v8) + dist + deploy script. Actually for frontend-only: pack only dist + use deploy script (which handles web dist swap separately).

---

## Live data for reference (Rocky 9 as of 2026-06-21)
- 301 devices in dim_device (297 eaton, 4 smartlogger)
- ~259 devices responding per cycle
- ~42 devices unreachable (10.10.6.x subnet, connect timeout)
- Poller cycle: 120s interval, 8 max concurrent
- v_device_health populated and updating live every 2 minutes
