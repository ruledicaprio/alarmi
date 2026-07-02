import { QdrantClient } from "@qdrant/js-client-rest";
import { createHash } from "node:crypto";
import { embedBatch } from "./embedder.js";

const COLLECTION = "alarm_knowledge";
const EMBEDDING_DIM = parseInt(process.env.EMBEDDING_DIM ?? "1024", 10);

export interface AlarmChunk {
  text: string;
  type: "site" | "active_alarm" | "event" | "alarm_class" | "neteco_alarm";
  site_key?: string;
  severity?: string;
  region?: string;
  source?: string;
  timestamp?: string;
}

export interface SearchResult {
  text: string;
  payload: Record<string, unknown>;
  score: number;
}

/** Deterministic UUID v4-shaped ID from content hash — guarantees idempotent upserts. */
function contentId(text: string): string {
  const h = createHash("sha256").update(text).digest("hex");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-4${h.slice(13, 16)}-a${h.slice(17, 20)}-${h.slice(20, 32)}`;
}

let _client: QdrantClient | null = null;
function client(): QdrantClient {
  if (!_client) {
    _client = new QdrantClient({ url: process.env.QDRANT_URL ?? "http://localhost:6333" });
  }
  return _client;
}

export async function ensureCollection(): Promise<void> {
  const { collections } = await client().getCollections();
  if (!collections.some((c) => c.name === COLLECTION)) {
    await client().createCollection(COLLECTION, {
      vectors: { size: EMBEDDING_DIM, distance: "Cosine" },
    });
    console.log(`[qdrant] Created collection '${COLLECTION}' (dim=${EMBEDDING_DIM})`);
  }
}

/** Remove all points of a given type — used to refresh active alarms on each cycle. */
export async function deleteByType(type: string): Promise<void> {
  await client().delete(COLLECTION, {
    filter: {
      must: [{ key: "type", match: { value: type } }],
    },
  });
}

/** Embed chunks and upsert into Qdrant. Idempotent: same content → same ID → overwrite. */
export async function upsertChunks(chunks: AlarmChunk[]): Promise<void> {
  if (chunks.length === 0) return;
  const texts = chunks.map((c) => c.text);
  const vectors = await embedBatch(texts);

  const points = chunks.map((chunk, i) => ({
    id: contentId(chunk.text),
    vector: vectors[i],
    payload: { ...chunk } as Record<string, unknown>,
  }));

  // Qdrant upsert in batches of 100 points
  for (let i = 0; i < points.length; i += 100) {
    await client().upsert(COLLECTION, {
      wait: true,
      points: points.slice(i, i + 100),
    });
  }
  console.log(`[qdrant] Upserted ${points.length} chunk(s)`);
}

export async function searchChunks(
  queryVec: number[],
  filter?: { severity?: string; type?: string },
  limit = 20,
): Promise<SearchResult[]> {
  const mustClauses = [
    ...(filter?.severity ? [{ key: "severity", match: { value: filter.severity } }] : []),
    ...(filter?.type ? [{ key: "type", match: { value: filter.type } }] : []),
  ];

  const results = await client().search(COLLECTION, {
    vector: queryVec,
    limit,
    ...(mustClauses.length > 0 ? { filter: { must: mustClauses } } : {}),
    with_payload: true,
  });

  return results.map((r) => ({
    text: (r.payload?.["text"] as string) ?? "",
    payload: (r.payload as Record<string, unknown>) ?? {},
    score: r.score,
  }));
}
