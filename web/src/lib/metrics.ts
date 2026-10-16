import type { StateSnapshot } from '../api/types.ts'
import { roomTitle, sensorRoom, zoneRooms } from './rooms.ts'

/** The device an input or output belongs to: "116.1" is sensor 1 of CAN device 116 (the °CALEONbox);
 *  a bare "1" is the thermostat's own sensor. */
export function deviceOf(label: string, state: StateSnapshot): string {
  const m = /^(\d+)\./.exec(label)
  if (!m) return 'Termostato'
  return state.devices[m[1]!] ?? `Equipo CAN ${m[1]}`
}

/** "116.1" -> "°CALEONbox 1", "1" -> "Termostato 1". */
export function deviceLabel(label: string, state: StateSnapshot): string {
  return `${deviceOf(label, state)} ${label.replace(/^\d+\./, '')}`
}

/** `{group, label, unit}` or a history key like `"Entradas · 116.1 [°C]"` → human name, or null to hide. */
export function metricName(
  m: string | { group: string; label: string; unit?: string },
  state: StateSnapshot,
): string | null {
  if (typeof m === 'string') {
    const mm = /^(?:(.*) · )?(.*?)(?: \[(.*)\])?$/.exec(m)
    m = { group: mm?.[1] || '', label: mm?.[2] ?? m, unit: mm?.[3] || '' }
  }
  const { group, label, unit } = m
  if (/--$/.test(label)) return null
  if (label === 'Exterior') return 'Temperatura exterior'
  if (group === 'Entradas') {
    const room = sensorRoom(label, state)
    if (room) return `${(unit || '').includes('%') ? 'Humedad' : 'Sensor'} de ${roomTitle(room)}`
    if (label === '1')
      return (unit || '').includes('%') ? 'Humedad del termostato' : 'Termostato °CALEON'
    if (/^\d+\.\d+$/.test(label)) return `Sonda ${deviceLabel(label, state)}`
    if (/^\d+\.N\d+$/.test(label)) return `Entrada PWM ${deviceLabel(label, state)}`
  }
  if (group.startsWith('Bomba circuito'))
    return (
      { 'Ida nominal': 'Impulsión objetivo', Ida: 'Impulsión', Modo: 'Bomba del circuito' }[label] ||
      label
    )
  if (group === 'Salidas') {
    const z = /Zona ([A-Z])$/.exec(label)
    if (z) {
      const r = zoneRooms(state)[z[1]]
      return r ? `${roomTitle(r)} · zona ${z[1]}` : null
    }
    if (/Bomba circuito/.test(label)) return 'Bomba del circuito'
    if (/Demanda de energía/.test(label)) return 'Demanda a la caldera'
  }
  return label
}
