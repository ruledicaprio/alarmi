import { Mastra } from "@mastra/core";
import { alarmAgent } from "./agents/alarm-agent.js";
import { startIndexer } from "./rag/indexer.js";

export const mastra = new Mastra({
  agents: { alarmAgent },
});

// Kick off the RAG indexer in the background — errors are caught internally
// and never crash the agent process.
startIndexer().catch((err) =>
  console.error("[mastra] Indexer init error:", err),
);
