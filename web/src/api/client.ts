export interface ApiOptions {
  method?: string
  body?: unknown
  /** When true, skip navigation stale checks (for global state updates). */
  any?: boolean
}

const hasErrorMessage = (v: unknown): v is { error: string } =>
  typeof v === 'object' && v !== null && 'error' in v && typeof v.error === 'string'

/** The JSON body of a reply; a non-2xx status becomes an Error carrying the server's message. */
export async function fetchJson(path: string, opts: ApiOptions = {}): Promise<unknown> {
  const hasBody = opts.body !== undefined
  const r = await fetch(path, {
    method: opts.method || (hasBody ? 'POST' : 'GET'),
    headers: hasBody ? { 'Content-Type': 'application/json' } : undefined,
    body: hasBody ? JSON.stringify(opts.body) : undefined,
  })
  const data: unknown = await r.json().catch(() => ({}))
  if (!r.ok) throw new Error((hasErrorMessage(data) && data.error) || r.statusText)
  return data
}

/**
 * The one place the wire shape is taken on trust. The handlers in crates/caleon-server/src/web.rs
 * serialise the structs that `cargo test export_types` writes to src/api/generated, so T describes
 * the reply rather than guessing it; there is no runtime schema library in the project to check it.
 */
export function api<T = unknown>(path: string, opts: ApiOptions = {}): Promise<T> {
  return fetchJson(path, opts) as Promise<T>
}
