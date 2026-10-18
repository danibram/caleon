import { MODE_LABEL, ROOM_TYPES, SYSTEM } from './constants.ts'
import type { Room, Snapshot } from '../context/AppContext.tsx'
import { es, fmtC, num } from './format.ts'

export const isDefaultName = (n: string | null | undefined) =>
  !n || /^habitaci[oó]n\s*\d+$/i.test(n.trim())

export const typeLabel = (t: string | null | undefined) => {
  const key = t || ''
  return ROOM_TYPES[key] || (/^room(\d+)$/.test(key) ? 'Habitación ' + Number(key.slice(4)) : key)
}

export const roomTitle = (r: Room) =>
  r.id === SYSTEM
    ? 'Toda la casa'
    : isDefaultName(r.name)
      ? typeLabel(r.type) || r.name
      : r.name

export const sensorRoom = (label: string, S: Snapshot | null): Room | null => {
  if (!S) return null
  const name = S.sensors[label]
  if (name == null) return null
  return S.rooms.find((r) => r.name === name) ?? null
}

export const zoneRooms = (S: Snapshot | null): Record<string, Room> => {
  const out: Record<string, Room> = {}
  if (!S) return out
  for (const [z, type] of Object.entries(S.zones)) {
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


/** What the room is doing (`text`, with `cls` heat | idle | wait | off | holiday), the mode it
 *  follows and the setpoint it aims for; the UI draws the last two after the activity. */
export type StateLine = { cls: string; text: string; mode?: string; target?: string }

export function stateLine(r: Room): StateLine {
  const mode = modeOf(r)
  const sp = num(r.tset)
  if (r.vacation) return { cls: 'holiday', text: `Vacaciones hasta ${es(r.vacation)}` }
  if (mode === 'off') return { cls: 'off', text: 'Modo apagado' }
  const cd = (r.mode || '').split(' ')[1]
  const n = (r.outputs || []).filter((o) => o.on).length
  const t = num(r.temperature)
  const rest = { mode: `${MODE_LABEL[mode] || mode}${cd ? ' ' + cd : ''}`, target: `${fmtC(sp)}°` }
  if (n) return { cls: 'heat', text: `Calentando${n > 1 ? ` · ${n} circuitos` : ''}`, ...rest }
  // on but idle: already warm enough, or below the target while the valve stays closed
  if (t !== null && sp !== null && t < sp) return { cls: 'wait', text: 'En espera', ...rest }
  return { cls: 'idle', text: 'A temperatura', ...rest }
}

/** The state line as one sentence, for screen readers. */
export function stateSentence(st: StateLine): string {
  return [st.text, st.mode, st.target ? `objetivo ${st.target}` : null].filter(Boolean).join(', ')
}

export function roomById(S: Snapshot | null, id: number): Room | undefined {
  if (!S) return undefined
  if (id === SYSTEM) {
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
  return S.rooms.find((r) => r.id === id)
}

