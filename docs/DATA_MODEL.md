# BHT Alarm Pipeline — Canonical Data Model (Stage 1)

The pipeline ingests three heterogeneous streams and collapses them onto **one**
event model keyed to **one** site inventory and **one** alarm taxonomy. Everything
downstream (Rust API, ant.design dashboard, history queries) speaks only this model.

## 1. The normalization problem

The same physical condition appears under different names per source:

| Physical condition | IgnitionSCADA | NetEco | U2020 | Eaton SC200/300 | HTML table |
|---|---|---|---|---|---|
| Loss of mains | *(varies)* | `Mains Failure`, `AC Phase L1 Undervoltage` | `Nestanak 220 V` | `AC_Fail` / code `1210` | — |
| Rectifier down | — | `Rectifier Power Failure` | `Kvar ispravljaca` | `Rectifier-Fail` `1205` | — |
| Element unreachable | `Gubitak komunikacije` | — | `NE Is Disconnected` | — | row present in section |

Normalization maps all of these to a single **`alarm_class`** (`MAINS_FAILURE`,
`RECTIFIER_FAILURE`, `NE_DISCONNECTED`, …).

## 2. Sources

| `source` | System | Stateful? | Notes |
|---|---|---|---|
| `ignition` | IgnitionSCADA (k8s) | no | severity feed (`Critical`/`Low`); count-only |
| `net_eco` | Huawei NetEco | no | raises only in feed; count-only |
| `u2020` | Huawei U2020 | yes | `major`/`critical`/`cleared`; dry-contact (door, AC fail, rectifier) |
| `rps_sc200` | Eaton SC200 (RPS-SC200-MIB) | yes | has region + device IP |
| `rps_sc300` | Eaton SC300 (RpsSc300Mib) | yes | has region + device IP |
| `dse74xx` | DSE 7410/7420 genset | yes | engine start/stop, genset alarms |
| `benning` | Benning rectifier (DCMCUMIB) | yes | `Added`/`Removed`; **timestamp is the last field** |
| `baran` | BARAN FCS cooling | yes | `poorcooling`, `fcsoff` |
| `modbus_eaton` | Direct Modbus poll | yes | **later stage**; emits the same `CanonicalEvent` |
| `html_oos` | `/alarmi/` out-of-service table | yes | one outage per row; technology = section |

**Stateful** sources emit both raise and clear, so RAISE→CLEAR durations pair.
Count-only sources (`ignition`, `net_eco`) become `INSTANT` events — counted, never
paired. (IgnitionSCADA alone is ~84% of all lines and its dominant alarm is
`Gubitak komunikacije` / comms-lost; treating it as count-only keeps duration
analytics meaningful instead of drowning in comms noise.)

## 3. CanonicalEvent

One normalized event = one row in `fact_event`:

| Field | Type | Meaning |
|---|---|---|
| `event_time` | `timestamptz` (UTC) | source local time (Sarajevo) → UTC |
| `source` | `source_t` | originating system (table above) |
| `site_key` | `text` | **canonical** site id (join key to `dim_site`) |
| `region` | `text` | region/city when the source provides it |
| `alarm_class` | `alarm_class_t` | normalized taxonomy (§4) |
| `severity` | `severity_t` | `critical`/`major`/`minor`/`warning`/`info` |
| `transition` | `transition_t` | `raise`/`clear`/`instant` (drives durations) |
| `raw_site`, `raw_alarm` | `text` | originals, kept for audit/triage |
| `device_ip` | `inet` | when present (SC200/300, DSE, Benning, BARAN) |

`site_key` canonicalization: upper-case, strip leading `BTS_/BS_/RRST_/RR_/DEA_/_DSE_`,
spaces/hyphens → `_`, collapse repeats. Example: `RR_Tusnica_Livno` → `TUSNICA_LIVNO`.

## 4. Alarm-class taxonomy

Ordered keyword rules (first match wins), covering English **and** Bosnian wording.
Power-critical classes are flagged in `dim_alarm_class.is_power_critical`.

`NE_DISCONNECTED · COMMS_LOST · MAINS_FAILURE · RECTIFIER_FAILURE · RECTIFIER_COMMS ·
SOLAR_FAULT · UPS_MODULE · BATTERY_LOW · BATTERY_FAULT · HIGH_VOLTAGE · GENSET_EVENT ·
COOLING_FAULT · DOOR_OPEN · FUSE_LOAD · GENERIC_ERROR · SERVICE_OUTAGE · UNCLASSIFIED`

Rules live in **two synchronized places**:
`tools/normalize_ref.py` (the validation oracle) and `crates/normalize/src/classify.rs`
(production). Adding a class = add one rule in both + one enum label in `db/schema.sql`.

## 5. Storage & retention tiers

| Tier | Object | Granularity | Retention | Purpose |
|---|---|---|---|---|
| Hot detail | `fact_event` (hypertable) | per event | **90 days**, compressed after 7d | the 30-day "fast view", drill-down |
| Durations | `fact_alarm_episode` | per RAISE→CLEAR | 2 years | outage length / performance |
| History | `cagg_event_daily` (continuous agg) | per day/site/class | **5 years** | basic device/site history queries |
| Fast rollup | `cagg_event_hourly` | per hour | 90 days | dashboard charts |

Episodes are built by `rebuild_episodes()` using gaps-and-islands pairing
(collapse repeated states, pair each raise with the next clear) — the same
first-raise-to-first-clear logic as the prior PowerShell engine, now in SQL.

## 6. Seams left for later stages

- **Modbus poller (Rust)**: emits `CanonicalEvent` with `source = modbus_eaton`;
  industrial polling concerns (staggered reads, per-device circuit breaker,
  register-group batching, unit-IDs, byte/word order, retry/backoff) are owned
  there — `modbusmap-sc200300.txt` + `alarmna_lista.json` are the register/alarm maps.
- **DST**: Stage 1 uses a fixed +02:00 (CEST) offset; a tz-aware conversion
  (`Europe/Sarajevo`) replaces the constant in `parse.rs` / `normalize_ref.py`.
- **Inventory enrichment**: `dim_site` is seeded from `neteco_sites.csv` (769 sites);
  lat/lon, genset/battery/solar flags and technologies come from `genset-inventory/`
  and `gis-map-export/`.
- **Ingest API**: Stage 1 bulk-loads via `COPY`; a streaming ingest endpoint
  (Rust/Axum) replaces the periodic curl+load loop.

## Update — live feed (2026-06): Ignition & NetEco are stateful

The live `/alarmi/ispadnap` feed carries an explicit **status field** that the
April sample lacked:
- **Ignition**: field 5 = `critical` (raise) / `cleared` (clear).
- **NetEco**: now a 5th field = `critical` / `cleared`.
- **DSE**: the **last** field is the status (`major` / `clear`), e.g.
  `notifMainsFail`→raise, `notifMainsReturn`→clear.

So transition is now **purely status-driven for every source** (no more
count-only special cases): `cleared/clear/normal/removed → CLEAR`,
`critical/major/minor/warning/low/active/added → RAISE`, else INSTANT. These
RAISE/CLEAR pairs feed `rebuild_episodes()` for duration analytics. `notifmains`
was added to the MAINS_FAILURE taxonomy so DSE mains fail/return pair to the same
class. Verified on a live sample: 100% parse + classify, correct pairing.
