# Stage 4 — ant.design dashboard

React + TypeScript + Ant Design + Recharts SPA in `web/`, consuming the Stage-3
API. Built to static files and served **by `bht-api` itself** (no nginx) — one
binary serves both `/api/*` and the UI.

## Pages
- **Overview** — stat cards (active alarms, events in window, sites), bar charts
  (events by class, by region), active-alarms table, time-window selector.
- **Alarms** — recent events with filters (window, source, class, site).
- **Sites** — all sites + open-alarm counts; click through to…
- **Site detail** — 30-day reliability (episodes / outage hours / avg) + a
  measurement chart (battery V, load kW, AC V, solar, energy) per metric.

## Build (on your PC / laptop — needs Node 18+)
```bash
cd web
npm install
npm run build        # -> web/dist
```
> Not built in the sandbox: running vite there hits a SIGBUS from the virtiofs
> mount (native esbuild/rollup binaries can't mmap off it). It builds normally on
> a real disk. The source is standard antd/recharts; nothing exotic.

## Dev (hot reload, proxies to a local API)
```bash
cd web && npm run dev          # http://localhost:5173, proxies /api -> :8080
# in another shell:
cargo run -p bht-api           # API on :8080
```

## Production (served by the API)
`bht-api` serves `static_dir` (default `web/dist`) with SPA fallback. So after
`npm run build`, just run the API and open it:
```bash
cargo run -p bht-api           # or the deployed binary on Rocky
# open http://<host>:8080/  -> dashboard;  /api/* -> data
```
If `web/dist` doesn't exist, the API runs API-only (no UI) — harmless.

## Deploy to Rocky
Build `web/dist` on your PC, ship it next to the binary:
```
/opt/bht/
  bht-api   config/api.toml
  web/dist/...        # copied from your build (set static_dir=/opt/bht/web/dist)
```
Point `static_dir` in `api.toml` at that path. One systemd service (`bht-api`)
then serves the whole dashboard on the isolated LAN.
