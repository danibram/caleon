// VITE_MOCK=1 pnpm dev: serves the /api fixtures in src/mocks, so the UI runs with no server or thermostat.
import { readFileSync } from 'node:fs'
import type { IncomingMessage, ServerResponse } from 'node:http'
import { join } from 'node:path'
import type { Plugin } from 'vite'

const MOCKS = join(import.meta.dirname, 'src/mocks')
const ICONS = join(import.meta.dirname, '../static/icons')

const fixture = (name: string): unknown => JSON.parse(readFileSync(join(MOCKS, `${name}.json`), 'utf8'))

type Row = [number, ...unknown[]]

/** Moves every timestamp forward so the captured data reads as current. */
function shifted() {
  const state = fixture('state') as Record<string, unknown> & { updated: number; history_since: number | null }
  const shift = Math.floor(Date.now() / 1000) - state.updated
  const rows = (series: Record<string, Row[]>) =>
    Object.fromEntries(
      Object.entries(series).map(([k, list]) => [k, list.map(([t, ...rest]) => [t + shift, ...rest])]),
    )
  return {
    state: {
      ...state,
      updated: state.updated + shift,
      history_since: state.history_since === null ? null : state.history_since + shift,
    },
    history: rows(fixture('history') as Record<string, Row[]>),
    metrics: rows(fixture('metrics') as Record<string, Row[]>),
  }
}

function icon(name: string, mask: boolean): string | null {
  let svg: string
  try {
    svg = readFileSync(join(ICONS, name.replace(/[^\w.-]/g, '')), 'utf8')
  } catch {
    return null
  }
  const style = mask
    ? '<style>.icon-background{fill:none}.icon-foreground{fill:#000}</style>'
    : '<style>.icon-background{fill:#f2612a}.icon-foreground{fill:#fff}</style>'
  const open = svg.indexOf('<svg')
  const at = open < 0 ? -1 : svg.indexOf('>', open)
  return at < 0 ? svg : svg.slice(0, at + 1) + style + svg.slice(at + 1)
}

const READ_ONLY = { error: 'Modo solo lectura: este servidor no puede cambiar el termostato' }

function route(method: string, path: string): [number, unknown] {
  if (method === 'GET') {
    if (path === '/api/state') return [200, shifted().state]
    if (path === '/api/history') return [200, shifted().history]
    if (path === '/api/metrics') return [200, shifted().metrics]
    if (path === '/api/log') return [200, fixture('log')]
    if (path === '/api/settings') return [200, fixture('settings')]
    if (path === '/api/backups') return [200, fixture('backups')]
    if (path === '/api/menu/values') return [200, fixture('menu-values')]
    if (path === '/api/menu/detail') return [200, (fixture('menu-root') as { detail: unknown }).detail]
    if (/^\/api\/rooms\/\d+\/timer$/.test(path)) return [200, fixture('timer')]
  }
  if (method === 'POST') {
    if (path === '/api/refresh') return [200, shifted().state]
    if (path === '/api/menu/open' || path === '/api/menu/action') return [200, fixture('menu-root')]
  }
  if (path.startsWith('/api/')) return [403, READ_ONLY]
  return [404, { error: 'not found' }]
}

export function mockApi(): Plugin {
  return {
    name: 'caleon-mock-api',
    configureServer(server) {
      server.middlewares.use((req: IncomingMessage, res: ServerResponse, next: () => void) => {
        const url = new URL(req.url ?? '/', 'http://mock')
        if (url.pathname.startsWith('/icons/')) {
          const svg = icon(decodeURIComponent(url.pathname.slice(7)), url.searchParams.has('mask'))
          if (svg === null) {
            res.statusCode = 404
            res.end()
          } else {
            res.setHeader('Content-Type', 'image/svg+xml')
            res.end(svg)
          }
          return
        }
        if (!url.pathname.startsWith('/api/')) return next()
        const [status, body] = route(req.method ?? 'GET', url.pathname)
        res.statusCode = status
        res.setHeader('Content-Type', 'application/json')
        res.end(JSON.stringify(body))
      })
    },
  }
}
