// Same-origin in production (served by bht-api); dev uses Vite proxy.
export async function api<T = any>(path: string): Promise<T> {
  const r = await fetch(path)
  if (!r.ok) throw new Error(`${r.status} ${r.statusText}`)
  return r.json()
}

export const sevColor: Record<string, string> = {
  critical: 'red', major: 'volcano', minor: 'orange', warning: 'gold', info: 'blue',
}
