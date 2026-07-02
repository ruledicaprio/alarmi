import { createTool } from "@mastra/core/tools";
import { z } from "zod";
import { embed, rerank, RankedDoc } from "../rag/embedder.js";
import { searchChunks } from "../rag/qdrant.js";

export const searchAlarmKnowledge = createTool({
  id: "search_alarm_knowledge",
  description:
    "Semantic search over indexed alarm history, site profiles, and active alarms. " +
    "Use this for: patterns ('what alarms usually happen at site X?'), historical analysis " +
    "('what happened in Tuzla last week?'), cross-site comparisons, or root-cause synthesis. " +
    "Use the live API tools instead when the question is about the current state right now.",
  inputSchema: z.object({
    query: z.string().describe("Natural language question or search phrase"),
    filter_severity: z
      .enum(["critical", "major", "minor", "warning", "info"])
      .optional()
      .describe("Restrict results to a specific severity level"),
    filter_type: z
      .enum(["active_alarm", "event", "site", "neteco_alarm", "alarm_class"])
      .optional()
      .describe("Restrict to a specific document type"),
    limit: z
      .number()
      .int()
      .min(1)
      .max(10)
      .default(5)
      .describe("Number of results to return after reranking"),
  }),
  execute: async ({ context }) => {
    const queryVec = await embed(context.query);

    const raw = await searchChunks(
      queryVec,
      { severity: context.filter_severity, type: context.filter_type },
      20, // retrieve 20, rerank down to limit
    );

    const docs: RankedDoc[] = raw.map((r) => ({
      text: r.text,
      payload: r.payload,
      score: r.score,
    }));

    const reranked = await rerank(context.query, docs);

    return {
      chunks: reranked.slice(0, context.limit ?? 5).map((d) => ({
        text: d.text,
        relevance_score: d.score,
        type: d.payload["type"],
        site_key: d.payload["site_key"],
        severity: d.payload["severity"],
        timestamp: d.payload["timestamp"],
        region: d.payload["region"],
      })),
      total_candidates: raw.length,
    };
  },
});
