# Integrating Huawei iMaster NetEco / iSitePower REST NBI for SMU02B & SMU02C Devices – Re‑evaluated Guide

**Context**  
You are building a Rust/Axum + TimescaleDB ingest stack for Huawei iMaster NetEco (iSitePower).  
The environment currently has **two SMU devices**: **SMU02B** and **SMU02C**.  
This guide re‑evaluates the generic NBI integration **exclusively** for these SMUs – no PV/ESS devices.

---

## TL;DR

- The NBI is **poll‑only**; no webhooks.  
- SMUs belong to the **iSitePower (site‑power)** product line. Their exact `devTypeId` and metric keys are **not** in the public PV‑focused NBI references.  
- **You must fingerprint the live instance first**: discover the SMU `devTypeId`, fetch one real‑time payload, and build your schema from that payload.  
- Use the same `reqwest` + token manager + cron scheduler + rate‑limiter architecture as the generic design.  
- For only two SMUs, rate limits are trivial.  
- Store SMU telemetry in a **dedicated wide hypertable** (all SMU02B/02C share the same field set).  
- Alarms are already covered by the existing `getAlarmList` loop.

---

## 1. Auth Dialect – Which One Are You On?

Your on‑prem iSitePower instance most likely uses one of two patterns:

- **Pattern B – NetEco‑1000S `/openApi/*` on port 27200**  
- **Pattern A – `/thirdData/*` with `XSRF‑TOKEN` header**

**Fingerprint immediately:**

```bash
# Test Pattern B (openApi)
curl -k -X POST 'https://<host>:27200/openApi/login' \
  -d 'userName=<northbound_user>&password=<password>' -i

# Test Pattern A (thirdData)
curl -k -X POST 'https://<host>/thirdData/login' \
  -H 'Content-Type: application/json' \
  -d '{"userName":"...","systemCode":"..."}' -i
```

Whichever returns a token/cookie is your dialect. This guide assumes **Pattern B** (openApi) because it’s the canonical iSitePower interface, but the discovery steps are identical for Pattern A – just replace the URIs accordingly.

---

## 2. Locate the SMU `devTypeId` and Real‑Time Endpoint

### 2.1 List all devices

**Pattern B (openApi):**  
`POST /openApi/queryDeviceList`  
Parameters: `stationCode=<code>&openApiroarand=<token>`  
(cookie `JSESSIONID` must be present)

**Pattern A (thirdData):**  
`POST /thirdData/getDevList`  
Body: `{"stationCodes":"<stationCode>"}`

Look for entries where `devName` contains `SMU02B` or `SMU02C` (or match by `esnCode`).  
Record:

- `devId`        – the numeric device ID
- `devTypeId`    – the integer device type (key!)
- `stationCode`  – the plant/site code

> The public `devTypeId` enumeration (1,2,8,10,…) does **not** include an SMU entry. Your system will assign a type ID – possibly 80, 90, 100, or another number. Note it exactly.

### 2.2 Fetch one real‑time SMU payload

**Pattern B** likely uses:  
`POST /openApi/queryDeviceRealtimeData`  
Parameters: `devIds=<id1,id2>&devTypeId=<type>&openApiroarand=<token>`

If that returns 404 or unknown API, try:

- `/openApi/queryDeviceKPI`
- `/openApi/getDevRealKpi`
- `/openApi/queryDeviceDetail` (might give only static info, but worth checking)

**Pattern A** would use:  
`POST /thirdData/getDevRealKpi`  
Body: `{"devTypeId":<type>,"devIds":"<id1,id2>"}`

**Save the full JSON response** – it is the authoritative schema.

*Example (speculative – your actual keys will differ):*

```json
{
  "success": true,
  "failCode": 0,
  "data": [
    {
      "devId": 500000000123,
      "sn": "SMU02B2106000123",
      "dataItemMap": {
        "dev_run_state": 1,
        "ac_input_voltage": 230.5,
        "dc_output_voltage": -53.5,
        "load_current": 12.3,
        "battery_voltage": 48.2,
        "battery_capacity": 85.0,
        "system_temp": 32.1,
        "smoke_alarm": 0,
        "door_status": 1
      }
    }
  ],
  "message": null,
  "params": { "currentTime": 1700000000000 }
}
```

From this payload you extract every key, its unit, and expected range.

---

## 3. TimescaleDB Schema for SMU Metrics (Wide Hypertable)

Because all SMU02B/02C share the same field set, a **per‑metric‑column hypertable** is ideal.

```sql
CREATE TABLE smu_metrics (
  ts               TIMESTAMPTZ NOT NULL,
  device_id        BIGINT      NOT NULL,
  station_code     TEXT        NOT NULL,
  dev_type_id      SMALLINT    NOT NULL,   -- the discovered SMU type ID
  source           TEXT        NOT NULL DEFAULT 'nbi_rest',

  -- Adjust these columns to exactly match the dataItemMap keys
  dev_run_state    SMALLINT,
  comm_status      SMALLINT,
  ac_input_voltage DOUBLE PRECISION,
  dc_output_voltage DOUBLE PRECISION,
  load_current     DOUBLE PRECISION,
  battery_voltage  DOUBLE PRECISION,
  battery_capacity DOUBLE PRECISION,
  system_temp      DOUBLE PRECISION,
  smoke_alarm      SMALLINT,
  door_status      SMALLINT,
  water_leak       SMALLINT,
  -- … add any other keys you discover

  PRIMARY KEY (device_id, ts)
);

SELECT create_hypertable('smu_metrics', 'ts', chunk_time_interval => INTERVAL '7 days');
```

### 3.1 Continuous aggregates

```sql
CREATE MATERIALIZED VIEW smu_metrics_5m
WITH (timescaledb.continuous) AS
SELECT
  time_bucket('5 minutes', ts) AS bucket,
  device_id,
  station_code,
  AVG(dc_output_voltage) AS avg_dc_v,
  MAX(load_current) AS max_load_a,
  AVG(system_temp) AS avg_temp,
  LAST(battery_capacity, ts) AS batt_cap_pct,
  MAX(smoke_alarm) AS smoke_triggered
FROM smu_metrics
GROUP BY 1, 2, 3;

SELECT add_continuous_aggregate_policy('smu_metrics_5m',
  start_offset => INTERVAL '1 day',
  end_offset => INTERVAL '5 minutes',
  schedule_interval => INTERVAL '5 minutes');
```

### 3.2 Compression & retention

```sql
ALTER TABLE smu_metrics SET (
  timescaledb.compress,
  timescaledb.compress_segmentby = 'device_id',
  timescaledb.compress_orderby = 'ts DESC'
);
SELECT add_compression_policy('smu_metrics', INTERVAL '14 days');
SELECT add_retention_policy('smu_metrics', INTERVAL '400 days');
```

---

## 4. Rust Ingest Service Integration

Your existing architecture (`reqwest` client, token manager, cron scheduler, `governor` rate limiter) is reused.  
Add these SMU‑specific steps:

### 4.1 Inventory cache
After `getDevList`/`queryDeviceList`, filter devices where `devTypeId` equals the SMU type.  
Store them in a `Vec<SmuDevice { id, station_code }>`.

### 4.2 Poll loop (every 5 minutes)
1. Check token age (refresh if >25 min for 30‑min XSRF, or similar for openApi).  
2. Take the list of SMU `devId`s (two devices) and call the real‑time endpoint:
   - Pattern B: `POST /openApi/queryDeviceRealtimeData` with `devIds=id1,id2&devTypeId=<type>&openApiroarand=<token>`
   - Pattern A: `POST /thirdData/getDevRealKpi` with JSON body.
3. Deserialize `dataItemMap` into your typed `SmuMetrics` struct using `serde`.  
4. Insert into `smu_metrics` (use `ON CONFLICT (device_id, ts) DO NOTHING` or simple INSERT if timestamps are unique).

### 4.3 Rate limits
With two SMUs of one type:  
`getDevRealKpi` limit = Roundup(2/100) = **1 call per 5 minutes**. That’s exactly your poll interval.  
No concurrency issues, no extra throttling needed.  
Login limit (5 calls / 10 min) is never hit by a single token refresh every 25 minutes.

### 4.4 Error handling
- `failCode 0` = success. Always inspect the JSON body, not just HTTP status.  
- `401` → token expired → re‑login (respect the 5‑logins/10min limit).  
- `407` → rate limit exceeded → back off (you shouldn’t see this for 2 devices).  
- `403/429` → system overload → 60‑second global cooldown.

---

## 5. SMU Alarms – Already Covered

Your existing `getAlarmList` polling loop (Pattern A: `/thirdData/getAlarmList`, Pattern B: equivalent openApi alarm endpoint) will automatically include SMU alarms.  
The `alarms` table schema you designed earlier works without changes – SMU alarms will have:

- `devName` = `SMU02B‑...` / `SMU02C‑...`
- `devTypeId` = the discovered SMU type ID
- alarm names like “SMU Communication Failure”, “Rectifier Failure”, “Battery Low Voltage” etc.

The SNMP‑vs‑REST reconciliation logic remains identical; SMU alarms are part of the same stream.

---

## 6. Day‑1 Execution Checklist

1. **Create a dedicated northbound user** in iMaster NetEco UI with permissions:  
   `Plant List`, `Real‑time device data`, `Alarm`.
2. **Fingerprint auth** – run the two curl commands from §1.
3. **Get device list** → find the two SMUs, record `devId` and `devTypeId`.
4. **Fetch one real‑time payload** → save full JSON.
5. **Create the `smu_metrics` table** with columns matching the `dataItemMap` keys exactly.
6. **Implement the Rust poll loop**:
   - Schedule every 5 minutes.
   - Batch both SMUs in one call.
   - Insert rows.
7. **Verify**:
   - Data appears in the hypertable.
   - Continuous aggregates update.
   - Alarms appear for SMU events.
8. **Monitor** – watch for `failCode` errors; adjust endpoint if needed.

---

## 7. Further Extensions

When you later add UPS, rectifier, HVAC, or generators, repeat the same discovery process per device type:

- Find `devTypeId` from device list.
- Pull one real‑time payload.
- Create a **new wide hypertable** for that device class (e.g., `ups_metrics`, `rectifier_metrics`).

The architecture scales cleanly: one poll loop per device type, each respecting its own per‑5‑minute rate limit.

---

**Remember:** The live payload is the authoritative specification. All keys and unit semantics come from your captured JSON, not from generic PV documentation.  
If the exact openApi real‑time endpoint is not found, consult the **SitePower NBI Reference** for your version (available through Huawei support) or your local Huawei account team.
