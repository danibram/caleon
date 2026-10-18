import type { Metric } from './generated/Metric.ts'
import type { Room as RoomBase } from './generated/Room.ts'
import type { RoomState } from './generated/RoomState.ts'
import type { Snapshot as SnapshotBase } from './generated/Snapshot.ts'

export type { Metric, RoomState }
export type { Settings } from './generated/Settings.ts'

/** The server flattens the room's RoomState into it; outdoor sensors have none. */
export type Room = RoomBase & Partial<RoomState>

export type RoomOutput = Room['outputs'][number]

export type StatusMetric = Metric

/** GET /api/state: the snapshot plus the fields `state()` in src/web.rs adds. */
export type Snapshot = Omit<SnapshotBase, 'rooms'> & {
  rooms: Room[]
  crawling: boolean
  crawl_done: number
  crawl_expected: number
  history_since: number | null
  backup_hours: number
  read_only: boolean
  /** room id -> mode -> last setpoint seen in that mode */
  mode_temps: Record<string, Record<string, number>>
  /** queries sent to the thermostat: since start, and in the last minute */
  device_calls?: { total: number; last_minute: number }
}

export type StateSnapshot = Snapshot

/** [ts, temperature, humidity, tset, mode] */
export type HistoryPoint = [number, number | null, number | null, number | null, string | null]

export type HistoryResponse = Record<string, HistoryPoint[]>

export type MetricPoint = [number, number | null]

export type MetricsResponse = Record<string, MetricPoint[]>
