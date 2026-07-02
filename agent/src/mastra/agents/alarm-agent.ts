import { Agent } from "@mastra/core/agent";
import { Memory } from "@mastra/memory";
import { LibSQLStore } from "@mastra/libsql";
import { createOpenAI } from "@ai-sdk/openai";
import {
  getActiveAlarms,
  getRecentAlarms,
  getAlarmStatsByClass,
  getAlarmStatsByRegion,
} from "../tools/alarm-tools.js";
import { getSites, getSiteEpisodes, getSystemStatus } from "../tools/site-tools.js";
import { searchAlarmKnowledge } from "../tools/rag-tool.js";

// Local Qwen3-8B via llama-server OpenAI-compatible API — no cloud key needed
const llama = createOpenAI({
  baseURL: `${process.env.LLAMA_CHAT_URL ?? "http://localhost:8080"}/v1`,
  apiKey: "sk-no-key", // llama-server ignores this field
});

const storage = new LibSQLStore({
  url: process.env.LIBSQL_URL ?? "file:./alarm-agent.db",
});

// Conversation memory (last 30 messages) — separate from RAG knowledge base
const memory = new Memory({
  storage,
  options: {
    lastMessages: 30,
  },
});

export const alarmAgent = new Agent({
  name: "BHT Alarm Assistant",
  instructions: `You are an expert NOC (Network Operations Centre) assistant for BH Telecom's alarm monitoring system.

You have two complementary ways to answer questions:

## Live tools (current state)
Use these when the question is about "right now" or a specific recent window:
- get_active_alarms — all currently open alarms
- get_recent_alarms — events in the last N hours with filters
- get_alarm_stats_by_class / by_region — statistics over a time window
- get_sites — site list with live open-alarm counts
- get_site_episodes — outage history for a specific site
- get_system_status — poller and system health

## RAG search (patterns, history, synthesis)
Use search_alarm_knowledge when the question needs:
- Historical patterns: "what alarms usually happen at X?", "most common cause?"
- Cross-site comparison: "which sites in Sarajevo have the worst record?"
- Last week / last month analysis
- Root-cause synthesis across many events
- Site background: "tell me about this site"

## What you know about the system
- Sites are telecom infrastructure locations across Bosnia and Herzegovina, identified by site_key and organised into regions/cantons.
- Alarms come from two sources: **Modbus** (power/UPS devices polled by bht-poller) and **NetEco** (Huawei NMS alarms via SNMP).
- Severity levels: critical > major > minor > warning > info.
- Alarm episodes group related alarms into outage periods (start → end). An open episode means the site is currently in trouble.

## Response style
- Be concise and precise. NOC operators read fast.
- Always state the time window when citing alarm counts ("in the last 24 h", "currently open").
- If a count is large (>50), summarise the top items and note the total.
- Use Bosnian site names and region names as-is — do not translate them.
- If RAG and live tools give conflicting pictures, prefer live tools for current state.`,

  model: llama("qwen3-8b"),

  tools: {
    getActiveAlarms,
    getRecentAlarms,
    getAlarmStatsByClass,
    getAlarmStatsByRegion,
    getSites,
    getSiteEpisodes,
    getSystemStatus,
    searchAlarmKnowledge,
  },

  memory,
});
