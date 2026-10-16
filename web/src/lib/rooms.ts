import { MODE_LABEL, ROOM_TYPES, SYSTEM } from './constants.ts'
import type { Room, Snapshot } from '../context/AppContext.tsx'
import { es, fmtC, num } from './format.ts'

export const isDefaultName = (n: string | null | undefined) =>
  !n || /^habitaci[oó]n\s*\d+$/i.test(n.trim())

export const typeLabel = (t: string | null | undefined) =>
  ROOM_TYPES[t || ''] ||
  (/^room(\d+)$/.test(t || '') ? 'Habitación ' + Number(t!.slice(4)) : t || '')

export const roomTitle = (r: Room) =>
  r.id === SYSTEM
    ? 'Toda la casa'
    : isDefaultName(r.name)
      ? typeLabel(r.type) || r.name
      : r.name

export const sensorRoom = (label: string, S: Snapshot | null): Room | null => {
  const n = S?.sensors?.[label]
  if (n == null) return null
  const name = typeof n === 'string' ? n : String(n)
  return S!.rooms.find((r) => r.name === name) ?? null
}

export const zoneRooms = (S: Snapshot | null): Record<string, Room> => {
  const out: Record<string, Room> = {}
  if (!S) return out
  for (const [z, t] of Object.entries(S.zones || {})) {
    const type = typeof t === 'string' ? t : String(t)
    const r = S.rooms.find((room) => room.type === type)
    if (r) out[z] = r
  }
  return out
}

export const modeOf = (r: Room) => (r.mode || '').split(' ')[0]!

export const isHeating = (r: Room) => (r.outputs || []).some((o) => o.on)

export function modeTemp(r: Room, mode: string, S: Snapshot | null): number | null {
  const v = S?.mode_temps?.[String(r.id)]?.[mode]
  return v === undefined ? null : v
}

export type StateLine = { cls: string; text: string }

export function stateLine(r: Room): StateLine {
  const mode = modeOf(r)
  const sp = num(r.tset)
  if (r.vacation) return { cls: 'holiday', text: `Vacaciones hasta ${es(r.vacation)}` }
  if (mode === 'off') return { cls: 'off', text: 'Apagada' }
  const cd = (r.mode || '').split(' ')[1]
  const base = `${MODE_LABEL[mode] || mode}${cd ? ' ' + cd : ''} · objetivo ${fmtC(sp)}°`
  const n = (r.outputs || []).filter((o) => o.on).length
  return n
    ? {
        cls: 'heat',
        text: `Calentando${n > 1 ? ` · ${n} circuitos` : ''} · objetivo ${fmtC(sp)}°`,
      }
    : { cls: mode, text: base }
}

export function roomById(S: Snapshot | null, id: number | string): Room | undefined {
  if (!S) return undefined
  if (id == SYSTEM || Number(id) === SYSTEM) {
    if (!S.system) return undefined
    return {
      id: SYSTEM,
      type: 'home',
      temperature: '',
      humidity: '',
      outputs: [],
      ...S.system,
      name: 'Todas las habitaciones',
    }
  }
  return S.rooms.find((r) => r.id == id)
}

