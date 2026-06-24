# NetEco iSitePower NBI Integration — Final Concrete Guide
## iMaster NetEco SitePower V600R025C30CP1005 × Rocky 9 (192.168.108.88) × alarmi/bht-api

---

## 1. The Ground Truth — From Your Live Signal Export

The CSVs exported from your NetEco instance reveal the exact device model, signal, and alarm catalog. No more guessing.

### 1.1 Device Taxonomy (Standard Types)

| Your Hardware | NetEco Standard Type Name | Standard Type ID | Role |
|--------------|--------------------------|-----------------|------|
| **SMU02B** | Controller | **60026** | Device gateway (model enum = 0: SMU02B) |
| **SMU02C** | Controller | **60026** | Device gateway (model enum = 1: SMU02C) |
| SMU operational signals | Site Unit | **60067** | Temperature, humidity, AC/DC power, energy |
| Entire power system | Power System | **69999** | Aggregate: voltage, load, supply type |
| **OPM** (Outdoor Power Module) | Power System | **69999** | Self-contained power system = Power System entity |
| Rectifier modules | Rectifier Group | **60039** + Rectifier **60040** | Per-module voltage/current/state |
| Battery bank | Battery Group | **60016** | SOC, SOH, voltage, current, backup time |
| AC mains input | Mains | **60001** | Grid V/I/F/P, mains state |
| **DPDU** (DC PDU) | DC Output Distribution | **60009** | DC bus voltage, total load current/power |
| AC input panel | AC Input Distribution | **60013** | Phase V/I/P/F + custom "Agregat u radu" DI |
| Generator | Genset | **60003** | Run state, fuel, runtime, V/I/P/T/RPM |
| Environment | Water Sensor, Door Sensor, UIM | various | DI alarms |

> **Critical clarification:** The Standard Type ID is used by the **WebService Signal Subscription** (Section 8.3.6.3). The `devTypeId` used by the **REST API** (`/thirdData/getDevRealKpi`) is a separate internal enum — fingerprint it from `getDevList`.

### 1.2 Controller (60026) — SMU02B/C Identity Signal

The Controller type has a `Controller Model` signal (ID 10010) that distinguishes your SMUs:
```
[0: SMU02B] [1: SMU02C] [2: SMU02S] [3: SMU02D] [9: SMU06T2]
[10: SMU02X_E] [11: SMU02C1] [21: SMU02C2] [7: SCC800_S1] ...
```
When parsing `getDevList`, match on `devTypeId` for Controller AND then check the model enum in the KPI payload to confirm SMU02B vs SMU02C.

### 1.3 Custom Live Signal in Your Export

Your signal export contains a site-specific custom signal on AC Input Distribution:
```
AC Input Distribution | 60013 | Agregat u radu | Signal ID: 60001 | Type: DI (TIME)
```
This is a dry-contact "Generator running" indicator — native Bosnian label, meaning it was entered in the NetEco UI by BH Telecom staff. This confirms your export is live and authoritative for this installation.

---

## 2. Integration Architecture (Corrected)

### 2.1 The Three NBI flows — what each actually does

```
┌──────────────────────────────────────────┐
│     NetEco iSitePower 10.10.0.3:31943    │
│                                          │
│  SMU02B/C ←→ Power System ←→ Battery    │
│  Mains ←→ DPDU ←→ Genset ←→ HVAC       │
└──────────────────────────────────────────┘
         │                    ▲
         │ SNMP Traps         │ REST Pull (your code calls NetEco)
         │ (active now)       │ /thirdData/* every 5 min
         ▼                    │
┌─────────────────────────────────────────────┐
│   Rocky 9   192.168.108.88                  │
│                                             │
│  bht-api (Axum, :8080)                      │
│  bht-poller (Modbus, Eaton SC200/300)        │
│  PostgreSQL 16 + TimescaleDB 2.28 (:5432)   │
└─────────────────────────────────────────────┘
```

**Flow A — SNMP Traps → Rocky 9** (already active): NetEco pushes alarm events. Keep this.

**Flow B — REST Pull (WebService NBI)**: Your new `neteco-poller` service calls NetEco's `/thirdData/*` API every 5 minutes for performance metrics, and every 1–2 minutes for alarm reconciliation. This is the **primary integration to build**.

**Flow C — OpenAPI Management**: NetEco calls YOUR OAuth endpoint and pushes data outbound. Optional, more complex — skip until B is stable.

> **There is no WebService NBI inbound HTTP push** (that was a misconception from earlier). The WebService NBI "third-party system" config in the UI registers your system as an authorized API caller and applies alarm filters to what the API returns. It's pull-only.

### 2.2 What the WebService NBI Third-Party System config actually does

When you click **Create** in `System > Northbound Integration > WebService NBI`:
- **IP address** → your Rocky 9 IP (`192.168.108.88`) — authorizes this source to call NetEco's REST API
- **System name** → `alarmi` or `bht-dashboard`
- **Enable alarm filtering** → filters what `getAlarmList` returns for this user
- **Filter Settings** → Severity, Alarm Category, Source Types — these are server-side query filters applied when your system calls `getAlarmList`

This is whitelist + filter configuration, not a push endpoint.

---

## 3. Authentication — Confirmed Details

### 3.1 Create the NBI user in NetEco

`System > System Management > User Management > Create`

| Setting | Value |
|---------|-------|
| User Type | **Third-party** (M2M, no web browser access) |
| Auto-logout | **30 minutes — hardcoded, cannot change** |
| Max online sessions | 1 |
| Account validity | Unlimited |
| Permissions needed | WebService NBI Configuration, Alarm read |

### 3.2 Auth flow

```
POST https://10.10.0.3:31943/thirdData/login
Content-Type: application/json
{"userName": "<nbi_user>", "systemCode": "<password>"}

Response header: XSRF-TOKEN: <token>  (valid 30 min)
```

Use the token as `XSRF-TOKEN: <value>` header on every data call. Refresh at 25-minute mark. Serialize logins to avoid hitting the 5-calls/10-min lockout.

---

## 4. REST API Endpoints & Exact Payloads

Base URL: `https://10.10.0.3:31943`

All calls: `POST`, JSON body, add header `XSRF-TOKEN: <token>`, TLS self-signed (use `-k` in curl, install NetEco CA in production).

### 4.1 Site inventory (daily cache)

```bash
# List all sites
POST /thirdData/getStationList
Body: {}
Response: { "data": [{ "stationCode": "...", "stationName": "...", ... }] }
```

### 4.2 Device list (daily cache, gives devTypeId)

```bash
POST /thirdData/getDevList
Body: { "stationCodes": "<stationCode>" }
Response: { "data": [{ "id": <devId>, "devName": "SMU02B-...", 
                       "devTypeId": ???, "esnCode": "...", "stationCode": "..." }] }
```

`devTypeId` here is the internal REST API enum (NOT the Standard Type ID from the CSV). Record the exact integer for SMU02B, SMU02C, DPDU, OPM.

### 4.3 Real-time metrics (every 5 minutes)

```bash
POST /thirdData/getDevRealKpi
Body: { "devTypeId": <internal_type_id>, "devIds": "<id1>,<id2>" }
Response: { "data": [{ "devId": <id>, "sn": "...",
            "dataItemMap": { "<key>": <value>, ... } }] }
```

The `dataItemMap` keys in the REST API are **different from the Standard Signal Names** in the CSV. The CSV uses human labels ("Indoor Temperature"). The REST API uses snake_case keys (e.g., `indoor_temperature` or `temperature_indoor`). You must fingerprint the live payload to get exact key names.

### 4.4 Alarm list (every 1–5 minutes)

```bash
POST /thirdData/getAlarmList
Body: { "stationCodes": "<code>", "status": 1 }   # status=1 = active alarms only
```

From the alarm CSV, Site Unit has **185 unique alarms** (including all power system events). Power System has **187**. These two types cover essentially all operational alarms for your equipment. Critical ones to watch for from the CSV:

**Site Unit / Power System (SMU alarms) — Critical/Major:**
- `Bus Bar Ultra Undervoltage` (ID:10155) 🔴 Critical
- `BLVD Warning` / `BLVD Low Voltage Disconnected` 🔴 Critical
- `Charge Abnormal` (ID:20816) 🔴 Critical
- `Load Breaker Trip` / `Main AC Breaker Trip` 🔴 Critical
- `SMU Fault` (ID:10216) 🟠 Major — SMU hardware fault
- `AC Failure` / `Long AC Failure` 🟠 Major — mains lost
- `Bus Bar Overvoltage / Undervoltage` 🟠 Major
- `LLVD Disconnected` (LVD1–7) 🟠 Major — load shedding
- `Indoor Over Temperature` (ID:61844) 🟡 Minor
- `Indoor Over Humidity` (ID:61846) 🟡 Warning

**Battery Group (51 unique alarms) — Critical/Major:**
- `Battery Breaker Trip` (ID:10721) 🔴
- `Battery Disconnected` (ID:11304) 🔴
- `BLVD Warning` (ID:11330) 🔴
- `Battery Undervoltage` (ID:10723) 🟠
- `SOH Low` (ID:11362) 🟠
- `BLVD High Temperature / Low Voltage Disconnected` 🟠

**Rectifier (11 unique alarms):**
- `Rectifier Fault (Non-redundant)` 🟠 — Critical if all modules fail
- `All Rectifiers Communication Failure` 🟠
- `Low Rectifier Remaining Capacity` 🔴

**DC Output Distribution / DPDU (112 unique alarms):**
- `DC Ultra Undervoltage` (ID:10155) 🔴
- `Load Breaker Trip` (ID:10250) 🔴
- `LLVD Disconnected` (1–7) 🟠
- `Load Fuse N Broken` (fuses 1–42) 🟠
- `DC SPD Fault` 🟠

---

## 5. TimescaleDB Schema — Exact Columns from Live Signal Export

Use the Standard Signal Names from the CSV as column semantics. Map to snake_case column names. Exact REST `dataItemMap` key names must be confirmed from live payload, but the semantics are locked from the CSV.

### 5.1 Existing infrastructure

```sql
-- Already exists (schema v8):
-- Postgres 16 + TimescaleDB 2.28.0 at localhost:5432
-- bht-api serves on port 8080
-- existing tables for Eaton/Modbus data

-- All new NetEco tables go in a new schema to avoid collision:
CREATE SCHEMA IF NOT EXISTS neteco;
```

### 5.2 Controller (SMU02B/02C device table — non-time-series)

```sql
CREATE TABLE neteco.devices (
    device_id      BIGINT PRIMARY KEY,         -- devId from getDevList
    station_code   TEXT NOT NULL,
    dev_name       TEXT,
    esn_code       TEXT,
    dev_type_id    INT,                         -- internal REST devTypeId
    std_type_id    INT,                         -- Standard Type ID from CSV (60026 for Controller)
    std_type_name  TEXT,                        -- 'Controller','Site Unit','Power System', etc.
    controller_model INT,                       -- 0=SMU02B, 1=SMU02C (from KPI payload)
    hw_version     TEXT,
    sw_version     TEXT,
    longitude      DOUBLE PRECISION,
    latitude       DOUBLE PRECISION,
    updated_at     TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE neteco.sites (
    station_code   TEXT PRIMARY KEY,
    station_name   TEXT,
    updated_at     TIMESTAMPTZ DEFAULT now()
);
```

### 5.3 Site Unit metrics (SMU02B/02C, Standard Type 60067)

Exact signals from your live export:

```sql
CREATE TABLE neteco.site_unit_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    
    -- Signal ID 10002: Indoor Temperature (°C)
    indoor_temp_c               DOUBLE PRECISION,
    -- Signal ID 10003: Outdoor Temperature (°C)
    outdoor_temp_c              DOUBLE PRECISION,
    -- Signal ID 10004: Indoor Humidity (%RH)
    indoor_humidity_pct         DOUBLE PRECISION,
    -- Signal ID 10007: AC Input Power (kW)
    ac_input_power_kw           DOUBLE PRECISION,
    -- Signal ID 10008: DC Output Power (kW)
    dc_output_power_kw          DOUBLE PRECISION,
    -- Signal ID 10011: AC Output Power (kW) [note: "AC Onput Power" typo in CSV]
    ac_output_power_kw          DOUBLE PRECISION,
    -- Signal ID 10009: Max BBU Temperature (°C)
    max_bbu_temp_c              DOUBLE PRECISION,
    -- Signal ID 10005: Total AC Input Energy Consumption (kWh)
    total_ac_input_energy_kwh   DOUBLE PRECISION,
    -- Signal ID 10006: Total DC Output Energy Consumption (kWh)
    total_dc_output_energy_kwh  DOUBLE PRECISION,
    -- Signal ID 10010: Staggering Exception Cause (ENUM)
    staggering_exception_cause  INT,
    
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.site_unit_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
ALTER TABLE neteco.site_unit_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.site_unit_metrics', INTERVAL '14 days');
```

### 5.4 Power System metrics (OPM, Standard Type 69999)

```sql
CREATE TABLE neteco.power_system_metrics (
    ts                              TIMESTAMPTZ NOT NULL,
    device_id                       BIGINT NOT NULL,
    station_code                    TEXT NOT NULL,
    
    -- Signal ID 10013: Current Power Supply Type (ENUM)
    -- 0=Unknown, 1=Mains, 2=DG, 3=Battery, 4=Mains+Battery, 5=DG+Battery, ...
    current_power_supply_type       INT,
    -- Signal ID 10016: DC Output Voltage (V)
    dc_output_voltage_v             DOUBLE PRECISION,
    -- Signal ID 10017: Total DC Load Current (A)
    total_dc_load_current_a         DOUBLE PRECISION,
    -- Signal ID 10018: Total DC Load Power (kW)
    total_dc_load_power_kw          DOUBLE PRECISION,
    -- Signal ID 10012: System Load Ratio (%)
    system_load_ratio_pct           DOUBLE PRECISION,
    -- Signal ID 10020: Total AC Input Energy Consumption (kWh)
    total_ac_input_energy_kwh       DOUBLE PRECISION,
    -- Signal ID 10019: Total DC Load Energy Consumption (kWh)
    total_dc_load_energy_kwh        DOUBLE PRECISION,
    -- Signal ID 10009: Total Temperature Control Energy (kWh)
    total_temp_control_energy_kwh   DOUBLE PRECISION,
    -- Signal ID 10014: 48V Port Current (A)
    port_48v_current_a              DOUBLE PRECISION,
    -- Signal ID 10015: 48V DC Load Current (A)
    dc_load_48v_current_a           DOUBLE PRECISION,
    
    source                          TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.power_system_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
ALTER TABLE neteco.power_system_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.power_system_metrics', INTERVAL '14 days');
```

### 5.5 DC Output Distribution metrics (DPDU, Standard Type 60009)

```sql
CREATE TABLE neteco.dpdu_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    
    -- Signal ID 10001: DC Output Voltage (V)
    dc_output_voltage_v         DOUBLE PRECISION,
    -- Signal ID 10002: Total DC Load Current (A)
    total_dc_load_current_a     DOUBLE PRECISION,
    -- Signal ID 10003: Total DC Load Power (kW)
    total_dc_load_power_kw      DOUBLE PRECISION,
    -- Signal ID 10004: Total DC Load Energy Consumption (kWh)
    total_dc_load_energy_kwh    DOUBLE PRECISION,
    -- Signal ID 10005: Other Power Input Current (A)
    other_power_input_current_a DOUBLE PRECISION,
    -- Signal ID 10006: Number of LLVD circuits
    num_llvd                    SMALLINT,
    
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.dpdu_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
ALTER TABLE neteco.dpdu_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.dpdu_metrics', INTERVAL '14 days');
```

### 5.6 Battery Group metrics (Standard Type 60016)

```sql
CREATE TABLE neteco.battery_group_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    
    -- Signal ID 10001: Battery State (ENUM: 0=Float, 1=Boost, 2=Discharging, ...)
    battery_state               INT,
    -- Signal ID 10002: Voltage (V)
    voltage_v                   DOUBLE PRECISION,
    -- Signal ID 10003: Current (A)
    current_a                   DOUBLE PRECISION,
    -- Signal ID 10004: Remaining Capacity Percent (SOC %)
    soc_pct                     DOUBLE PRECISION,
    -- Signal ID 10016: SOH (%)
    soh_pct                     INT,
    -- Signal ID 10005: Temperature (°C)
    temp_c                      DOUBLE PRECISION,
    -- Signal ID 10007: Remaining Backup Time (h)
    backup_time_h               DOUBLE PRECISION,
    -- Signal ID 10028: Remaining Backup Time AI (h)
    backup_time_ai_h            DOUBLE PRECISION,
    -- Signal ID 10026: Charge/Discharge Power (kW)
    charge_discharge_power_kw   DOUBLE PRECISION,
    -- Signal ID 10013: Total Rated Capacity (Ah)
    rated_capacity_ah           INT,
    -- Signal ID 10014: Remaining Capacity (Ah)
    remaining_capacity_ah       INT,
    -- Signal ID 10008: Total Cycle Times
    total_cycle_times           INT,
    -- Signal ID 10009: Current Limiting State (ENUM)
    current_limiting_state      INT,
    -- Signal ID 10011: On/Off State (ENUM)
    on_off_state                INT,
    
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.battery_group_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
ALTER TABLE neteco.battery_group_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.battery_group_metrics', INTERVAL '14 days');
```

### 5.7 Mains metrics (Standard Type 60001)

```sql
CREATE TABLE neteco.mains_metrics (
    ts                      TIMESTAMPTZ NOT NULL,
    device_id               BIGINT NOT NULL,
    station_code            TEXT NOT NULL,
    
    -- Signal ID 10001: Mains State (0=Off, 1=On)
    mains_state             INT,
    -- Signal ID 10002: AC Voltage (V)
    ac_voltage_v            DOUBLE PRECISION,
    phase_l1_v              DOUBLE PRECISION,  -- 10003
    phase_l2_v              DOUBLE PRECISION,  -- 10004
    phase_l3_v              DOUBLE PRECISION,  -- 10005
    -- Signal ID 10006: AC Current (A)
    ac_current_a            DOUBLE PRECISION,
    phase_l1_a              DOUBLE PRECISION,  -- 10007
    phase_l2_a              DOUBLE PRECISION,  -- 10008
    phase_l3_a              DOUBLE PRECISION,  -- 10009
    -- Signal ID 10010: Active Power (kW)
    active_power_kw         DOUBLE PRECISION,
    -- Signal ID 10011: AC Frequency (Hz)
    ac_freq_hz              DOUBLE PRECISION,
    -- Signal ID 10021: Power Factor
    power_factor            DOUBLE PRECISION,
    -- Signal ID 10016: Total Energy Consumption (kWh)
    total_energy_kwh        DOUBLE PRECISION,
    -- Signal ID 10020: Grid Quality Grade (ENUM: 0=Unknown, 1-4)
    grid_quality_grade      INT,
    
    source                  TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.mains_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
ALTER TABLE neteco.mains_metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id',
    timescaledb.compress_orderby = 'ts DESC');
SELECT add_compression_policy('neteco.mains_metrics', INTERVAL '14 days');
```

### 5.8 AC Input Distribution (Standard Type 60013)

```sql
CREATE TABLE neteco.ac_input_metrics (
    ts                      TIMESTAMPTZ NOT NULL,
    device_id               BIGINT NOT NULL,
    station_code            TEXT NOT NULL,
    
    -- Signal ID 10013: AC Input State (0=Failure, 1=Normal)
    ac_input_state          INT,
    phase_l1_v              DOUBLE PRECISION,
    phase_l2_v              DOUBLE PRECISION,
    phase_l3_v              DOUBLE PRECISION,
    phase_l1_a              DOUBLE PRECISION,
    phase_l2_a              DOUBLE PRECISION,
    phase_l3_a              DOUBLE PRECISION,
    ac_freq_hz              DOUBLE PRECISION,
    active_power_kw         DOUBLE PRECISION,
    apparent_power_kva      DOUBLE PRECISION,
    power_factor            DOUBLE PRECISION,
    total_energy_kwh        DOUBLE PRECISION,
    -- Signal ID 60001: "Agregat u radu" (custom DI — generator running)
    -- DI type, TIME in CSV (this is a boolean DI contact state, store as int 0/1)
    agregat_u_radu          INT,
    
    source                  TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.ac_input_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
```

### 5.9 Genset metrics (Standard Type 60003)

```sql
CREATE TABLE neteco.genset_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    
    -- Signal ID 10003: Running State (0=Unknown, 1=Stopped, 2=Running)
    running_state               INT,
    -- Signal ID 10007: Load Rate (%)
    load_rate_pct               DOUBLE PRECISION,
    -- Signal ID 10008: Cabin Temperature (°C)
    cabin_temp_c                DOUBLE PRECISION,
    -- Signal ID 10017: Coolant Temperature (°C)
    coolant_temp_c              DOUBLE PRECISION,
    -- Signal ID 10014: Oil Pressure (bar)
    oil_pressure_bar            DOUBLE PRECISION,
    -- Signal ID 10015: Oil Level (%)
    oil_level_pct               INT,
    -- Signal ID 10011: Rotation Speed (RPM)
    rotation_speed_rpm          INT,
    -- Signal ID 10018: Output Power (kW)
    output_power_kw             DOUBLE PRECISION,
    -- Signal ID 10019: AC Frequency (Hz)
    ac_freq_hz                  DOUBLE PRECISION,
    phase_l1_v                  DOUBLE PRECISION,
    phase_l2_v                  DOUBLE PRECISION,
    phase_l3_v                  DOUBLE PRECISION,
    phase_l1_a                  DOUBLE PRECISION,
    phase_l2_a                  DOUBLE PRECISION,
    phase_l3_a                  DOUBLE PRECISION,
    -- Signal ID 10004: Total Runtime (h)
    total_runtime_h             DOUBLE PRECISION,
    -- Signal ID 10005: Total Fuel Consumption (L)
    total_fuel_l                DOUBLE PRECISION,
    -- Signal ID 10040: Estimated Runtime with Remaining Fuel (h)
    estimated_runtime_h         DOUBLE PRECISION,
    -- Signal ID 10010: Total Energy Yield (kWh)
    total_energy_yield_kwh      DOUBLE PRECISION,
    -- Signal ID 10013: Genset Battery Voltage (V)
    genset_battery_voltage_v    DOUBLE PRECISION,
    
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.genset_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
```

### 5.10 Rectifier Group (Standard Type 60039)

```sql
CREATE TABLE neteco.rectifier_group_metrics (
    ts                          TIMESTAMPTZ NOT NULL,
    device_id                   BIGINT NOT NULL,
    station_code                TEXT NOT NULL,
    
    qty_rectifiers              INT,           -- 10001 (pcs)
    total_dc_output_current_a   DOUBLE PRECISION, -- 10002
    total_dc_output_power_kw    DOUBLE PRECISION, -- 10009
    load_usage_rate_pct         DOUBLE PRECISION, -- 10010 (%)
    output_voltage_v            DOUBLE PRECISION, -- 10011
    total_input_power_kw        DOUBLE PRECISION, -- 10012
    total_input_energy_kwh      DOUBLE PRECISION, -- 10003
    
    source                      TEXT NOT NULL DEFAULT 'nbi_rest',
    PRIMARY KEY (device_id, ts)
);
SELECT create_hypertable('neteco.rectifier_group_metrics', 'ts',
    chunk_time_interval => INTERVAL '7 days');
```

### 5.11 Alarms table (all sources unified)

```sql
CREATE TABLE neteco.alarms (
    alarm_id            TEXT PRIMARY KEY,      -- platform alarmId or synthetic key
    station_code        TEXT,
    station_name        TEXT,
    device_id           BIGINT,
    dev_name            TEXT,
    dev_type_id         INT,                   -- internal REST devTypeId
    std_type_id         INT,                   -- Standard Type ID (from alarm CSV)
    std_type_name       TEXT,                  -- 'Site Unit', 'Battery Group', etc.
    alarm_name          TEXT,
    alarm_cause         TEXT,
    alarm_id_number     INT,                   -- numeric alarm ID from alarm CSV
    alarm_type          SMALLINT,              -- 1=signal 2=exception 3=protection
    severity            SMALLINT,              -- 1=critical 2=major 3=minor 4=warning
    status              SMALLINT,              -- 1=active 2=acked 4=handled 5=user-clear 6=auto-clear
    raise_time          TIMESTAMPTZ,
    repair_time         TIMESTAMPTZ,
    source              TEXT NOT NULL,         -- 'snmp' | 'nbi_rest'
    first_seen          TIMESTAMPTZ DEFAULT now(),
    last_seen           TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX neteco_alarms_station ON neteco.alarms(station_code);
CREATE INDEX neteco_alarms_active ON neteco.alarms(severity) WHERE status = 1;
CREATE INDEX neteco_alarms_raise ON neteco.alarms(raise_time DESC);
```

### 5.12 Continuous Aggregates (key dashboard queries)

```sql
-- 5-minute rollup for live dashboard charts
CREATE MATERIALIZED VIEW neteco.site_unit_5m
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('5 minutes', ts) AS bucket,
    device_id, station_code,
    AVG(indoor_temp_c)          AS avg_indoor_temp_c,
    MAX(indoor_temp_c)          AS max_indoor_temp_c,
    AVG(outdoor_temp_c)         AS avg_outdoor_temp_c,
    AVG(indoor_humidity_pct)    AS avg_humidity_pct,
    AVG(ac_input_power_kw)      AS avg_ac_power_kw,
    AVG(dc_output_power_kw)     AS avg_dc_power_kw
FROM neteco.site_unit_metrics
GROUP BY 1, 2, 3;

SELECT add_continuous_aggregate_policy('neteco.site_unit_5m',
    start_offset => INTERVAL '1 day', end_offset => INTERVAL '5 minutes',
    schedule_interval => INTERVAL '5 minutes');

-- Battery SOC trend
CREATE MATERIALIZED VIEW neteco.battery_5m
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('5 minutes', ts) AS bucket,
    device_id, station_code,
    AVG(soc_pct)                AS avg_soc_pct,
    MIN(soc_pct)                AS min_soc_pct,
    LAST(soc_pct, ts)           AS last_soc_pct,
    LAST(battery_state, ts)     AS last_state,
    AVG(backup_time_h)          AS avg_backup_time_h,
    AVG(charge_discharge_power_kw) AS avg_cd_power_kw
FROM neteco.battery_group_metrics
GROUP BY 1, 2, 3;
```

---

## 6. Rust Poller Design

### 6.1 New service: `neteco-poller`

Add as a new systemd unit on Rocky 9, separate from `bht-poller`. It shares the same Postgres instance.

```toml
# In alarmi workspace Cargo.toml
[workspace]
members = ["backend", "frontend", "common", "neteco-poller"]

# neteco-poller/Cargo.toml
[dependencies]
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }  # rustls not native-tls (no openssl dep)
tokio = { version = "1", features = ["full"] }
tokio-cron-scheduler = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls", "chrono"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

### 6.2 TLS note for Rocky 9

NetEco on-prem ships with a self-signed certificate. For the poller:

```rust
// Production: install NetEco CA cert
let ca_cert = std::fs::read("/etc/ssl/neteco/neteco-ca.crt")?;
let cert = reqwest::Certificate::from_pem(&ca_cert)?;
let client = reqwest::Client::builder()
    .add_root_certificate(cert)
    .build()?;

// Development only (never in production systemd unit):
// .danger_accept_invalid_certs(true)
```

Or simpler: add the NetEco self-signed cert to Rocky 9's trust store:
```bash
cp neteco-ca.crt /etc/pki/ca-trust/source/anchors/
update-ca-trust
```
After that, `reqwest` with `rustls-tls` will trust it automatically.

### 6.3 Core poll loop

```rust
use tokio_cron_scheduler::{JobScheduler, Job};

async fn run_neteco_poller(client: Arc<NetEcoClient>, pool: Arc<PgPool>) {
    let sched = JobScheduler::new().await.unwrap();

    // Topology refresh — every 6h and on startup
    let c = client.clone(); let p = pool.clone();
    sched.add(Job::new_async("0 0 */6 * * *", move |_, _| {
        let (c, p) = (c.clone(), p.clone());
        Box::pin(async move { refresh_topology(&c, &p).await.ok(); })
    }).unwrap()).await.unwrap();

    // All device metrics — every 5 minutes
    let c = client.clone(); let p = pool.clone();
    sched.add(Job::new_async("30 */5 * * * *", move |_, _| {
        let (c, p) = (c.clone(), p.clone());
        Box::pin(async move { poll_all_device_metrics(&c, &p).await.ok(); })
    }).unwrap()).await.unwrap();

    // Active alarm reconciliation — every 2 minutes
    let c = client.clone(); let p = pool.clone();
    sched.add(Job::new_async("15 */2 * * * *", move |_, _| {
        let (c, p) = (c.clone(), p.clone());
        Box::pin(async move { poll_alarms(&c, &p).await.ok(); })
    }).unwrap()).await.unwrap();

    sched.start().await.unwrap();
    tracing::info!("NetEco poller started");
    std::future::pending::<()>().await;
}

async fn poll_all_device_metrics(client: &NetEcoClient, pool: &PgPool) -> anyhow::Result<()> {
    let devices = load_device_cache(pool).await?;
    // Group by devTypeId — one REST call per type per 5-min window
    let by_type = group_by_type(&devices);
    for (dev_type_id, ids) in by_type {
        for chunk in ids.chunks(100) {
            let ids_str = chunk.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
            match client.post_nbi("/thirdData/getDevRealKpi", &serde_json::json!({
                "devTypeId": dev_type_id,
                "devIds": ids_str
            })).await {
                Ok(resp) => {
                    for record in resp["data"].as_array().unwrap_or(&vec![]) {
                        upsert_device_metrics(pool, dev_type_id, record).await.ok();
                    }
                }
                Err(e) => tracing::warn!("KPI poll error devType={}: {}", dev_type_id, e),
            }
        }
    }
    Ok(())
}
```

### 6.4 Route dataItemMap to correct table

```rust
// Populate these constants from your live getDevList fingerprinting
const SITE_UNIT_TYPE_ID: i32 = ???;   // from live getDevList
const POWER_SYSTEM_TYPE_ID: i32 = ???;
const CONTROLLER_TYPE_ID: i32 = ???;
const BATTERY_GROUP_TYPE_ID: i32 = ???;
const MAINS_TYPE_ID: i32 = ???;
const DPDU_TYPE_ID: i32 = ???;
const RECTIFIER_GROUP_TYPE_ID: i32 = ???;
const GENSET_TYPE_ID: i32 = ???;
const AC_INPUT_TYPE_ID: i32 = ???;

async fn upsert_device_metrics(pool: &PgPool, dev_type_id: i32, record: &serde_json::Value)
    -> anyhow::Result<()>
{
    let dev_id = record["devId"].as_i64().unwrap_or(0);
    let map = record["dataItemMap"].as_object().cloned().unwrap_or_default();

    // Helper: extract float from dataItemMap, handle null
    let f = |key: &str| -> Option<f64> {
        map.get(key).and_then(|v| v.as_f64())
    };
    let i = |key: &str| -> Option<i32> {
        map.get(key).and_then(|v| v.as_i64()).map(|x| x as i32)
    };

    // Get station_code from device cache
    let station_code = get_station_code(pool, dev_id).await?;
    let now = chrono::Utc::now();

    match dev_type_id {
        t if t == SITE_UNIT_TYPE_ID => {
            // Exact key names TBD from live payload fingerprinting
            // Semantics known from CSV (Signal ID → column mapping above)
            sqlx::query!(
                "INSERT INTO neteco.site_unit_metrics
                 (ts, device_id, station_code,
                  indoor_temp_c, outdoor_temp_c, indoor_humidity_pct,
                  ac_input_power_kw, dc_output_power_kw, ac_output_power_kw,
                  max_bbu_temp_c, total_ac_input_energy_kwh, total_dc_output_energy_kwh,
                  staggering_exception_cause)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                 ON CONFLICT (device_id, ts) DO NOTHING",
                now, dev_id, station_code,
                f("indoor_temperature"),      // key TBD from live payload
                f("outdoor_temperature"),
                f("indoor_humidity"),
                f("ac_input_power"),
                f("dc_output_power"),
                f("ac_output_power"),
                f("max_bbu_temperature"),
                f("total_ac_input_energy"),
                f("total_dc_output_energy"),
                i("staggering_exception_cause"),
            ).execute(pool).await?;
        }
        // ... other match arms per type
        _ => {
            tracing::debug!("Unknown devTypeId {} — skipping", dev_type_id);
        }
    }
    Ok(())
}
```

### 6.5 failCode handling

```rust
fn handle_fail_code(fail_code: i64, client: &NetEcoClient) {
    match fail_code {
        0    => { /* success */ }
        401  => {
            tracing::warn!("Token invalid — forcing re-login");
            client.invalidate_token();  // clears cached token, next call re-logs-in
        }
        407  => {
            tracing::warn!("Rate limit hit (407) — backing off 5 min");
            // Implement per-endpoint backoff
        }
        403 | 429 => {
            tracing::warn!("System overload ({}) — 60s global cooldown", fail_code);
        }
        _ => {
            tracing::error!("NBI failCode {}: check NetEco logs", fail_code);
        }
    }
}
```

---

## 7. Day-1 Execution — Specific Steps for This Environment

### 7.1 On NetEco (10.10.0.3:31943, logged in as admin)

1. **Create NBI user**: `System > System Management > User Management > Create`
   - Type: **Third-party**
   - Note username + password (store in Rocky 9 env file, not in code)

2. **Register in WebService NBI**: `System > Northbound Integration > WebService NBI > Create`
   - IP: `192.168.108.88`
   - System name: `alarmi`
   - Enable alarm filtering: **Enable**
   - Filter mode: **Report** (only forward matching alarms)
   - Effective mode: **Any criterion**
   - Severity: **Critical + Major** (start here, add Minor after testing)
   - Alarm category: **New alarm, Clear alarm, Acknowledge alarm**
   - Maintenance Status: **Normal**

3. **Subscribe WebService signals**: `System > Northbound Integration > NBI Signal Management > WebService Northbound Signal Subscription`
   - Interface Parameter Setting → set Signal Data Model to **Standardized**
   - Subscribe: SMU02B, SMU02C, and other devices in the device tree

### 7.2 From Rocky 9 (or via SSH from your laptop)

```bash
# Step 1 — Test auth
TOKEN=$(curl -sk -X POST 'https://10.10.0.3:31943/thirdData/login' \
  -H 'Content-Type: application/json' \
  -d '{"userName":"alarmi_nbi","systemCode":"<password>"}' \
  -D - 2>/dev/null | grep -i XSRF-TOKEN | awk '{print $2}' | tr -d '\r')
echo "Token: $TOKEN"

# Step 2 — List sites
curl -sk -X POST 'https://10.10.0.3:31943/thirdData/getStationList' \
  -H "XSRF-TOKEN: $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{}' | python3 -m json.tool > /tmp/neteco_sites.json
cat /tmp/neteco_sites.json

# Step 3 — Device list (use stationCode from above)
STATION=$(python3 -c "import json; d=json.load(open('/tmp/neteco_sites.json')); print(d['data'][0]['stationCode'])")
curl -sk -X POST 'https://10.10.0.3:31943/thirdData/getDevList' \
  -H "XSRF-TOKEN: $TOKEN" \
  -H 'Content-Type: application/json' \
  -d "{\"stationCodes\":\"$STATION\"}" | python3 -m json.tool > /tmp/neteco_devices.json

# Step 4 — Extract devTypeId → device type mapping (CRITICAL)
python3 -c "
import json
data = json.load(open('/tmp/neteco_devices.json'))
types = {}
for d in data.get('data', []):
    t = d.get('devTypeId')
    name = d.get('devName','?')
    if t not in types:
        types[t] = []
    types[t].append(name)
for t, names in sorted(types.items()):
    print(f'devTypeId={t}: {names[:3]}')
"

# Step 5 — Get real-time KPI for each discovered devTypeId
# (replace <TYPE_ID> and <DEV_IDS> with values from Step 4)
curl -sk -X POST 'https://10.10.0.3:31943/thirdData/getDevRealKpi' \
  -H "XSRF-TOKEN: $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"devTypeId":<TYPE_ID>,"devIds":"<DEV_ID_1>,<DEV_ID_2>"}' \
  | python3 -m json.tool > /tmp/neteco_kpi_type<TYPE_ID>.json

# Step 6 — Extract dataItemMap keys (these are your Rust struct field names)
python3 -c "
import json
data = json.load(open('/tmp/neteco_kpi_type<TYPE_ID>.json'))
for record in data.get('data', []):
    print(f'devId: {record[\"devId\"]}')
    for k, v in record.get('dataItemMap', {}).items():
        print(f'  {k}: {v}')
    break  # one device is enough for schema discovery
"

# Step 7 — Active alarms
curl -sk -X POST 'https://10.10.0.3:31943/thirdData/getAlarmList' \
  -H "XSRF-TOKEN: $TOKEN" \
  -H 'Content-Type: application/json' \
  -d "{\"stationCodes\":\"$STATION\",\"status\":1}" \
  | python3 -m json.tool > /tmp/neteco_active_alarms.json
python3 -c "
import json
data = json.load(open('/tmp/neteco_active_alarms.json'))
print(f'Active alarms: {len(data.get(\"data\", []))}')
for a in data.get('data', [])[:5]:
    print(f'  [{a.get(\"lev\")}] {a.get(\"alarmName\")} @ {a.get(\"devName\")}')
"
```

### 7.3 Systemd unit for neteco-poller

```ini
# /etc/systemd/system/neteco-poller.service
[Unit]
Description=NetEco NBI Poller
After=postgresql-16.service network-online.target
Wants=network-online.target

[Service]
Type=simple
User=rusmir
WorkingDirectory=/home/rusmir
EnvironmentFile=/home/rusmir/.neteco.env
ExecStart=/home/rusmir/neteco-poller
Restart=on-failure
RestartSec=30

[Install]
WantedBy=multi-user.target
```

```bash
# /home/rusmir/.neteco.env (chmod 600)
NETECO_URL=https://10.10.0.3:31943
NETECO_USER=alarmi_nbi
NETECO_PASSWORD=<password>
DATABASE_URL=postgres://rusmir@localhost/alarmi_db
```

---

## 8. Integration with Existing bht-api

The existing `bht-api` already serves alarm and device data from the Eaton Modbus stack. NetEco data goes into the `neteco` schema on the same Postgres instance.

Add new Axum routes to `bht-api` (or a sidecar) that query the `neteco` schema:

```rust
// New dashboard endpoints in bht-api
GET /api/neteco/sites                   → neteco.sites
GET /api/neteco/devices                 → neteco.devices
GET /api/neteco/alarms/active           → neteco.alarms WHERE status=1
GET /api/neteco/metrics/site-unit/:id   → neteco.site_unit_5m (recent 24h)
GET /api/neteco/metrics/power/:id       → neteco.power_system_metrics
GET /api/neteco/metrics/battery/:id     → neteco.battery_5m
GET /api/neteco/metrics/mains/:id       → neteco.mains_metrics
```

The existing Proxmox dashboard "Ignition section" endpoints can reference these to give a unified view of both Eaton (SC200/300 via Modbus) and Huawei (SitePower via NBI) infrastructure.

---

## 9. Key Numbers to Remember

| Item | Value |
|------|-------|
| NetEco UI | https://10.10.0.3:31943 |
| Rocky 9 bht-api | http://192.168.108.88:8080 |
| Token validity | 30 minutes (hardcoded, cannot change) |
| Safe token refresh | at 25-minute mark |
| Login rate limit | 5 calls / 10 minutes per user |
| Metric granularity | 5 minutes minimum (server-side) |
| Site Unit (SMU) Standard Type | **60067** |
| Power System (OPM) Standard Type | **69999** |
| DPDU Standard Type | **60009** |
| Controller (SMU device) Standard Type | **60026** |
| Battery Group Standard Type | **60016** |
| Mains Standard Type | **60001** |
| AC Input Distribution Standard Type | **60013** |
| Genset Standard Type | **60003** |

---

*Generated 2025-06 from live signal export (1651 signals, 7771 alarm records), iMaster NetEco SitePower V600R025C30CP1005 client/admin guides, and BH Telecom / alarmi stack context.*
