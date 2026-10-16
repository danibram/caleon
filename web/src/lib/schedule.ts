import { DAYS, MODE_LABEL } from './constants.ts'
import type { Room, Snapshot } from '../context/AppContext.tsx'
import { modeOf, modeTemp } from './rooms.ts'
import { fmtC } from './format.ts'

export function hhmm(i: number): string {
  return `${String(Math.floor(i / 2)).padStart(2, '0')}:${i % 2 ? '30' : '00'}`
}

export function segments(slots: string[]) {
  const out: { mode: string; start: number; end: number }[] = []
  slots.forEach((m, i) => {
    const last = out[out.length - 1]
    if (last && last.mode === m) last.end = i + 1
    else out.push({ mode: m, start: i, end: i + 1 })
  })
  return out
}

export const todayIdx = () => (new Date().getDay() + 6) % 7

export const nowSlotIdx = () => {
  const d = new Date()
  return d.getHours() * 2 + (d.getMinutes() >= 30 ? 1 : 0)
}

export type NextChange =
  | { kind: 'change'; now: string; mode: string; slot: number; dayOffset: number }
  | { kind: 'steady'; now: string }

export function nextChange(r: Room): NextChange | null {
  if (!r.week) return null
  const today = todayIdx()
  const ns = nowSlotIdx()
  const cur = r.week[today][ns]
  for (let k = 1; k <= 48 * 7; k++) {
    const abs = today * 48 + ns + k
    const d = Math.floor(abs / 48) % 7
    const i = abs % 48
    const m = r.week[d][i]
    if (m !== cur) {
      return {
        kind: 'change',
        now: cur,
        mode: m,
        slot: i,
        dayOffset: Math.floor((today * 48 + ns + k) / 48) - today,
      }
    }
  }
  return { kind: 'steady', now: cur }
}

export function modeWithTemp(r: Room, mode: string, S: Snapshot | null): string {
  const t = modeTemp(r, mode, S)
  return `${MODE_LABEL[mode] || mode}${t !== null ? ` ${fmtC(t)}°` : mode === 'off' ? '' : ' · sin dato aún'}`
}

export type BandCaption = {
  caption: string
  paused: boolean
}

export function bandCaption(r: Room, big = false): BandCaption {
  if (!r.week) return { caption: '', paused: false }
  const nc = nextChange(r)
  const paused = modeOf(r) === 'off' || !!r.vacation
  let caption: string
  if (paused) {
    caption = r.vacation
      ? 'Programa detenido durante las vacaciones'
      : big
        ? 'Programa de hoy, detenido mientras esté apagada'
        : 'Programa detenido'
  } else if (nc?.kind === 'change') {
    const when =
      nc.dayOffset === 0
        ? 'a las'
        : nc.dayOffset === 1
          ? 'mañana a las'
          : DAYS[(todayIdx() + nc.dayOffset) % 7].toLowerCase() + ' a las'
    const next = `${MODE_LABEL[nc.mode] || nc.mode} ${when} ${hhmm(nc.slot)}`
    caption = `Ahora ${(MODE_LABEL[nc.now] || '').toLowerCase()} · ${next}`
  } else {
    caption = `${MODE_LABEL[nc?.now || ''] || ''} todo el día`
  }
  return { caption, paused }
}

/** Presets for vacation datetime-local input. */
export function vacationPresets(): [string, string][] {
  const localInput = (d: Date) => {
    const p = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`
  }
  const at = (days: number, h: number) => {
    const d = new Date()
    d.setDate(d.getDate() + days)
    d.setHours(h, 0, 0, 0)
    return d
  }
  const sunday = ((7 - new Date().getDay()) % 7) || 7
  return [
    ['Mañana', localInput(at(1, 18))],
    ['Este domingo', localInput(at(sunday, 18))],
    ['Una semana', localInput(at(7, 18))],
    ['Dos semanas', localInput(at(14, 18))],
  ]
}
