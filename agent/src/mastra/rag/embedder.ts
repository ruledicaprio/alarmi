const EMBED_URL = () =>
  `${process.env.LLAMA_EMBEDDING_URL ?? "http://localhost:8081"}/v1/embeddings`;
const RERANK_URL = () =>
  `${process.env.LLAMA_RERANKER_URL ?? "http://localhost:8082"}/v1/rerank`;

const EMBED_MODEL = "qwen3-embedding";
const RERANK_MODEL = "qwen3-reranker";
const BATCH_SIZE = 32;

interface EmbedResponse {
  data: Array<{ embedding: number[]; index: number }>;
}
interface RerankResponse {
  results: Array<{ index: number; relevance_score: number }>;
}

export interface RankedDoc {
  text: string;
  payload: Record<string, unknown>;
  score?: number;
}

export async function embed(text: string): Promise<number[]> {
  const res = await fetch(EMBED_URL(), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ model: EMBED_MODEL, input: text }),
  });
  if (!res.ok) throw new Error(`Embedding failed: HTTP ${res.status}`);
  const data = (await res.json()) as EmbedResponse;
  return data.data[0].embedding;
}

export async function embedBatch(texts: string[]): Promise<number[][]> {
  const results: number[][] = [];
  for (let i = 0; i < texts.length; i += BATCH_SIZE) {
    const batch = texts.slice(i, i + BATCH_SIZE);
    const res = await fetch(EMBED_URL(), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: EMBED_MODEL, input: batch }),
    });
    if (!res.ok) throw new Error(`Batch embedding failed: HTTP ${res.status}`);
    const data = (await res.json()) as EmbedResponse;
    const sorted = [...data.data].sort((a, b) => a.index - b.index);
    results.push(...sorted.map((d) => d.embedding));
  }
  return results;
}

/** Reranks docs by relevance to query. Falls back to original order on any error. */
export async function rerank(query: string, docs: RankedDoc[]): Promise<RankedDoc[]> {
  if (docs.length === 0) return docs;
  try {
    const res = await fetch(RERANK_URL(), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: RERANK_MODEL,
        query,
        documents: docs.map((d) => d.text),
      }),
    });
    if (!res.ok) {
      console.warn(`[reranker] HTTP ${res.status} — using original order`);
      return docs;
    }
    const data = (await res.json()) as RerankResponse;
    return data.results
      .sort((a, b) => b.relevance_score - a.relevance_score)
      .map((r) => ({ ...docs[r.index], score: r.relevance_score }));
  } catch (err) {
    console.warn("[reranker] Unavailable — using original order:", err);
    return docs;
  }
}
