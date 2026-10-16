export interface ApiOptions {
  method?: string
  body?: unknown
  /** When true, skip navigation stale checks (for global state updates). */
  any?: boolean
}

export async function api<T = unknown>(path: string, opts: ApiOptions = {}): Promise<T> {
  const init: RequestInit = {
    method: opts.method || (opts.body !== undefined ? 'POST' : 'GET'),
    headers: {},
  }
  if (opts.body !== undefined) {
    ;(init.headers as Record<string, string>)['Content-Type'] = 'application/json'
    init.body = JSON.stringify(opts.body)
  }
  const r = await fetch(path, init)
  const data = (await r.json().catch(() => ({}))) as { error?: string }
  if (!r.ok) throw new Error(data.error || r.statusText)
  return data as T
}
