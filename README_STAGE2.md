# Stage 2 — Rust Modbus poller (Eaton SC200/300)

Ports the Python `modbus_working.py` reference to an async Rust collector that
runs **on the Rocky 9 LXC** (192.168.108.88), which routes to the Modbus VLAN
(`ping 10.10.1.17` → 1.7 ms). Emits canonical alarm events + telemetry into the
Stage-1 TimescaleDB. Additive: no Stage-1 file was modified.

## What it does each cycle

For every enabled Eaton device (297 of 300; 3 SmartLoggers deferred):
1. **Status** — read discrete inputs 1001–1004 → Critical/Major/Minor/Warning.
2. **Alarms** — read discrete segments `1101–1107, 1201–1272, 1301–1304` in 8-bit
   blocks + coils `1–12`; each set bit → its `AlarmDef` (name/class/severity).
3. **Measurements** — input registers (2 regs, big-endian f32): `u_battery_v`,
   `p_load_kw`, `ac_voltage_v`, plus FNE sites `p_solar_kw`, `e_total_kwh`, `e_load_kwh`.
4. **Edge detection** — diff active set vs. last poll → `RAISE`/`CLEAR`
   `CanonicalEvent`s (`source = modbus_eaton`), which pair into episodes via the
   Stage-1 `rebuild_episodes()` automatically.
5. **Write** — batch INSERT measurements → `fact_measurement`, events → `fact_event`.

## Industrial polling hygiene

- **Async** (tokio + tokio-modbus), bounded concurrency (`max_concurrent`) via a
  semaphore, **staggered starts** spread across the poll interval (no thundering herd).
- **Per-device circuit breaker**: opens after N consecutive failures, half-open
  probe after cooldown — a dead site stops wasting the cycle budget.
- **Per-read timeout + retries with backoff**; TCP segment reads chunked like the
  reference. `base0` addressing honored (all BHT devices use base0=false).

## Layout

```
crates/poller/src/
  decode.rs    f32-BE / u32 / i32 / summary-status / addressing   (+unit tests)
  types.rs     config model; class/severity deserialize into Stage-1 enums
  profile.rs   register layout + alarm segments + coil map
  breaker.rs   circuit breaker                                      (+unit tests)
  state.rs     edge detection -> RAISE/CLEAR events                 (+unit test)
  poll.rs      async poll of one device (tokio-modbus)
  sink.rs      dry-run printer OR batched TimescaleDB writer
  main.rs      scheduler: stagger + concurrency + breaker + shutdown
config/
  poller.toml         runtime config (interval, timeouts, breaker, db dsn)
  devices.toml        300 devices generated from modbus/plcs.json
  eaton_alarms.toml   62 discrete + 12 coil alarms (name->class->severity)
db/schema_stage2.sql  fact_measurement hypertable + daily rollup (additive)
deploy/bht-poller.service  systemd unit for the Rocky LXC
```

## Run

On your home PC (dry-run, no devices needed — exercises config + decoders):
```bash
cargo test -p bht-poller          # decode + breaker + edge-detection unit tests
cargo build --release -p bht-poller
./target/release/bht-poller --dry-run --once     # parses configs, would-poll log
```

On the Rocky LXC (real devices + DB). Apply the additive schema first:
```bash
docker exec -i bht_tsdb psql -U bht -d alarms < db/schema_stage2.sql   # or psql on the box
./bht-poller --once               # one real cycle into TimescaleDB
# then run continuously via systemd (deploy/bht-poller.service)
```

Verify after a live cycle:
```sql
SELECT metric, count(*), round(avg(value)::numeric,2) FROM fact_measurement GROUP BY 1;
SELECT site_key, alarm_class, transition, count(*) FROM fact_event
  WHERE source='modbus_eaton' GROUP BY 1,2,3 ORDER BY 4 DESC LIMIT 20;
SELECT rebuild_episodes();        -- pairs the new modbus RAISE/CLEAR into episodes
```

## Seams for later

- **SmartLogger** (3 devices, unit 0, holding registers, u32 scaling) — skipped
  with a log line; the holding-register path from the Python reference slots in.
- **Edge-state persistence**: alarm state is in-memory; a restart re-RAISEs active
  alarms. Persisting last state (small table) removes that.
- **DST**: timestamps use `Utc::now()` at poll time (correct); historical parse
  in Stage 1 still uses a fixed +02:00 offset (its own seam).
