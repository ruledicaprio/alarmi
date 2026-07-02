import { createTool } from "@mastra/core/tools";
import { z } from "zod";

const apiBase = () => process.env.BHT_API_URL ?? "http://192.168.108.88:8080";

async function apiFetch(path: string): Promise<unknown> {
  const res = await fetch(`${apiBase()}${path}`);
  if (!res.ok) throw new Error(`bht-api ${path} → HTTP ${res.status}`);
  return res.json();
}

/** All alarms currently open (max 500, sorted by raised_at). */
export const getActiveAlarms = createTool({
  id: "get_active_alarms",
  description:
    "Return all currently active (open) alarms. Each item has: site_key, source, alarm_class, severity (critical/major/minor/warning/info), raised_at, open_minutes. Use this to answer 'what is alarming right now?'",
  inputSchema: z.object({}),
  execute: async () => apiFetch("/api/alarms/active"),
});

/** Recent alarm events — raised or cleared — with rich filter support. */
export const getRecentAlarms = createTool({
  id: "get_recent_alarms",
  description:
    "Return recent alarm events (raised + cleared) within a time window. Supports filtering by site, alarm class, severity, and source. Each item has: event_time, site_key, alarm_class, severity, transition (raised/cleared), source, raw_alarm, device_ip, region.",
  inputSchema: z.object({
    hours: z.number().int().min(1).max(720).default(24).describe("How many hours back to look (default 24, max 720)"),
    site: z.string().optional().describe("Filter to a specific site_key"),
    severity: z.enum(["critical", "major", "minor", "warning", "info"]).optional().describe("Filter by severity"),
    source: z.string().optional().describe("Filter by alarm source (e.g. 'modbus', 'neteco')"),
    alarm_class: z.string().optional().describe("Filter by alarm class/type"),
    limit: z.number().int().min(1).max(500).default(100).describe("Max rows to return"),
  }),
  execute: async ({ context: { hours, site, severity, source, alarm_class, limit } }) => {
    const params = new URLSearchParams();
    params.set("hours", String(hours ?? 24));
    params.set("limit", String(limit ?? 100));
    if (site) params.set("site", site);
    if (severity) params.set("severity", severity);
    if (source) params.set("source", source);
    if (alarm_class) params.set("class", alarm_class);
    return apiFetch(`/api/alarms/recent?${params}`);
  },
});

/** Alarm counts broken down by alarm class/type. */
export const getAlarmStatsByClass = createTool({
  id: "get_alarm_stats_by_class",
  description:
    "Return alarm event counts grouped by alarm class/type for a time window. Use to answer 'what types of alarms are most common?' or 'which alarm class fires most?'",
  inputSchema: z.object({
    hours: z.number().int().min(1).max(720).default(24).describe("Time window in hours"),
  }),
  execute: async ({ context: { hours } }) =>
    apiFetch(`/api/stats/by-class?hours=${hours ?? 24}`),
});

/** Alarm counts broken down by region/canton. */
export const getAlarmStatsByRegion = createTool({
  id: "get_alarm_stats_by_region",
  description:
    "Return alarm event counts grouped by geographic region. Use to answer 'which region has the most alarms?' or compare regions.",
  inputSchema: z.object({
    hours: z.number().int().min(1).max(720).default(24).describe("Time window in hours"),
  }),
  execute: async ({ context: { hours } }) =>
    apiFetch(`/api/stats/by-region?hours=${hours ?? 24}`),
});
