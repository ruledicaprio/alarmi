import { createTool } from "@mastra/core/tools";
import { z } from "zod";

const apiBase = () => process.env.BHT_API_URL ?? "http://192.168.108.88:8080";

async function apiFetch(path: string): Promise<unknown> {
  const res = await fetch(`${apiBase()}${path}`);
  if (!res.ok) throw new Error(`bht-api ${path} → HTTP ${res.status}`);
  return res.json();
}

/** Paginated site list with live open-alarm counts. */
export const getSites = createTool({
  id: "get_sites",
  description:
    "Return a list of monitored sites with their current open alarm count and worst severity. Supports filtering by region or search query. Use to find which sites are in trouble or to look up a site by name.",
  inputSchema: z.object({
    query: z.string().optional().describe("Search by site_key or display name (partial match)"),
    region: z.string().optional().describe("Filter by region/canton"),
    min_open: z.number().int().min(0).default(0).describe("Only return sites with at least this many open alarms"),
    limit: z.number().int().min(1).max(200).default(50).describe("Max sites to return"),
  }),
  execute: async ({ context: { query, region, min_open, limit } }) => {
    const params = new URLSearchParams();
    params.set("limit", String(limit ?? 50));
    if (query) params.set("q", query);
    if (region) params.set("region", region);
    if (min_open) params.set("min_open", String(min_open));
    return apiFetch(`/api/sites?${params}`);
  },
});

/** Alarm episodes (continuous outage periods) for a specific site. */
export const getSiteEpisodes = createTool({
  id: "get_site_episodes",
  description:
    "Return alarm episodes (start→end outage periods) for a specific site. Episodes group related alarms into a single incident timeline. Use to understand the history of a site's reliability.",
  inputSchema: z.object({
    site_key: z.string().describe("The site identifier, e.g. 'BIH_TK_001'"),
  }),
  execute: async ({ context: { site_key } }) =>
    apiFetch(`/api/sites/${encodeURIComponent(site_key)}/episodes`),
});

/** Overall system health and poller status. */
export const getSystemStatus = createTool({
  id: "get_system_status",
  description:
    "Return system status: poller health, last poll timestamps, DB connectivity, and event ingestion rates. Use to check if the monitoring system itself is healthy.",
  inputSchema: z.object({}),
  execute: async () => apiFetch("/api/system/status"),
});
