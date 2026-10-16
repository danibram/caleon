import type { HistoryResponse } from '../api/types.ts'
import type { Room, Snapshot } from '../context/AppContext.tsx'
import { modeOf } from './rooms.ts'
import { nextChange, type NextChange } from './schedule.ts'

export type HomeInsights = {
  offSince?: number
  trend?: number
}

export function insights(hist: HistoryResponse | null, S: Snapshot | null): HomeInsights {
  const out: HomeInsights = {}
  if (!hist || !S) return out
  const rooms = S.rooms.filter((r) => r.type !== 'outdoor')
  if (rooms.every((r) => modeOf(r) === 'off')) {
    let since = 0
    for (const r of rooms) {
      const rows = hist[String(r.id)] || []
      for (let i = rows.length - 1; i > 0; i--) {
        if (rows[i]![4] === 'off' && rows[i - 1]![4] !== 'off') {
          since = Math.max(since, rows[i]![0]!)
          break
        }
      }
    }
    if (since) out.offSince = since
  }
  const avgAt = (ts: number) => {
    const v = rooms
      .map((r) => (hist[String(r.id)] || []).find((x) => x[0]! >= ts))
      .filter(Boolean)
      .map((x) => x![1])
      .filter((x): x is number => x !== null)
    return v.length ? v.reduce((a, b) => a + b, 0) / v.length : null
  }
  const now = Date.now() / 1000
  const a = avgAt(now - 7200)
  const b = avgAt(now - 300)
  const first = Math.min(...rooms.map((r) => (hist[String(r.id)] || [[now]])[0]![0]!))
  if (a !== null && b !== null && now - first > 3000) {
    out.trend = (b - a) / Math.min(2, (now - Math.max(first, now - 7200)) / 3600)
  }
  return out
}

export type UpcomingChange = { r: Room; nc: Extract<NextChange, { kind: 'change' }> }

export function upcomingChangeToday(rooms: Room[]): UpcomingChange | undefined {
  const out: UpcomingChange[] = []
  for (const r of rooms) {
    if (modeOf(r) === 'off' || r.vacation) continue
    const nc = nextChange(r)
    if (nc?.kind === 'change' && nc.dayOffset === 0) out.push({ r, nc })
  }
  return out.sort((a, b) => a.nc.slot - b.nc.slot)[0]
}
