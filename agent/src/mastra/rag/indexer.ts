import { AlarmChunk, deleteByType, ensureCollection, upsertChunks } from "./qdrant.js";

const BHT_API = () => process.env.BHT_API_URL ?? "http://192.168.108.88:8080";

async function apiFetch<T>(path: string): Promise<T> {
  const res = await fetch(`${BHT_API()}${path}`);
  if (!res.ok) throw new Error(`bht-api ${path} → HTTP ${res.status}`);
  return res.json() as Promise<T>;
}

// ---------------------------------------------------------------------------
// Document builders — one function per data source

async function indexSites(): Promise<void> {
  const data = await apiFetch<{ items: Record<string, unknown>[] }>("/api/sites?limit=5000");
  const chunks: AlarmChunk[] = data.items.map((s) => ({
    text: `Site ${s["site_key"]}: ${s["name"] || s["site_key"]}, region ${s["region"] || "unknown"}, municipality ${s["municipality"] || "unknown"}. Currently ${s["open_alarms"]} open alarm(s), worst severity: ${s["worst_severity"] ?? "none"}.`,
    type: "site",
    site_key: s["site_key"] as string,
    region: s["region"] as string | undefined,
  }));
  await upsertChunks(chunks);
  console.log(`[indexer] sites: ${chunks.length}`);
}

async function indexActiveAlarms(): Promise<void> {
  await deleteByType("active_alarm");
  const data = await apiFetch<{ items: Record<string, unknown>[] }>("/api/alarms/active");
  const chunks: AlarmChunk[] = data.items.map((a) => ({
    text: `ACTIVE ALARM at ${a["site_key"]}: ${a["alarm_class"]} [${a["severity"]}] from source ${a["source"]}. Raised at ${a["raised_at"]}, open for ${Math.round(a["open_minutes"] as number)} minutes.`,
    type: "active_alarm",
    site_key: a["site_key"] as string,
    severity: a["severity"] as string,
    source: a["source"] as string,
    timestamp: a["raised_at"] as string,
  }));
  await upsertChunks(chunks);
  console.log(`[indexer] active alarms: ${chunks.length}`);
}

async function indexRecentEvents(hours: number): Promise<void> {
  const data = await apiFetch<{ items: Record<string, unknown>[] }>(
    `/api/alarms/recent?hours=${hours}&limit=500`,
  );
  const chunks: AlarmChunk[] = data.items.map((e) => ({
    text: `${e["event_time"]}: ${e["alarm_class"]} ${e["transition"]} [${e["severity"]}] at ${e["site_key"]} (${e["region"] || "unknown region"}), source: ${e["source"]}. Raw alarm: ${e["raw_alarm"] || "N/A"}.`,
    type: "event",
    site_key: e["site_key"] as string,
    severity: e["severity"] as string,
    region: e["region"] as string | undefined,
    source: e["source"] as string,
    timestamp: e["event_time"] as string,
  }));
  await upsertChunks(chunks);
  console.log(`[indexer] events (last ${hours}h): ${chunks.length}`);
}

async function indexAlarmClasses(): Promise<void> {
  // Response shape can be an array or { items: [] } depending on API version
  const raw = await apiFetch<unknown>("/api/stats/by-class?hours=168");
  const items: Record<string, unknown>[] = Array.isArray(raw)
    ? raw
    : ((raw as Record<string, unknown>)["items"] as Record<string, unknown>[]) ?? [];
  const chunks: AlarmChunk[] = items.map((c) => ({
    text: `Alarm class "${c["alarm_class"] ?? c["class"]}": ${c["count"] ?? c["total"]} events in the last 7 days.`,
    type: "alarm_class",
  }));
  await upsertChunks(chunks);
  console.log(`[indexer] alarm classes: ${chunks.length}`);
}

async function indexNetEcoAlarms(): Promise<void> {
  const raw = await apiFetch<unknown>("/api/neteco/alarms");
  const items: Record<string, unknown>[] = Array.isArray(raw)
    ? raw
    : ((raw as Record<string, unknown>)["items"] as Record<string, unknown>[]) ?? [];
  const chunks: AlarmChunk[] = items.map((a) => ({
    text: `NetEco alarm at ${a["station_name"] ?? a["site_key"]}: ${a["alarm_name"]} — ${a["alarm_cause"] ?? "no cause"} [severity ${a["severity"]}].`,
    type: "neteco_alarm",
    site_key: (a["station_code"] ?? a["site_key"]) as string,
    severity: String(a["severity"]),
    timestamp: a["raise_time"] as string | undefined,
  }));
  await upsertChunks(chunks);
  console.log(`[indexer] neteco alarms: ${chunks.length}`);
}

// ---------------------------------------------------------------------------
// Index orchestration

async function runIndex(isFirstRun: boolean): Promise<void> {
  const label = isFirstRun ? "full" : "incremental";
  console.log(`[indexer] Starting ${label} index run...`);
  try {
    if (isFirstRun) {
      // Full index: everything. Takes ~15-30s depending on dataset size.
      await indexSites();
      await indexAlarmClasses();
      await indexRecentEvents(168); // last 7 days
      await indexNetEcoAlarms();
    }
    // Active alarms always refreshed (delete + re-add)
    await indexActiveAlarms();
    if (!isFirstRun) {
      // Incremental: only index the last hour of new events (idempotent by content hash)
      await indexRecentEvents(1);
    }
    console.log(`[indexer] ${label} index complete`);
  } catch (err) {
    // Never crash the agent process — RAG degrades gracefully if unavailable
    console.error(`[indexer] ${label} index error:`, err);
  }
}

/** Call once at startup. Starts the 15-minute refresh loop. */
export async function startIndexer(): Promise<void> {
  try {
    await ensureCollection();
    await runIndex(true);
  } catch (err) {
    console.error(
      "[indexer] Startup failed (Qdrant or bht-api unreachable). RAG unavailable until next retry:",
      err,
    );
  }
  // Continue scheduling even if startup failed — next tick may succeed
  setInterval(() => runIndex(false), 15 * 60 * 1000);
}
