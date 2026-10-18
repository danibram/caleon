import { useState } from 'react'
import type { Room, Snapshot } from '../api/types.ts'
import { useApp } from '../context/AppContext.tsx'
import { useRoomControlGuard } from '../hooks/useRoomChange.ts'
import { MODE_LABEL } from '../lib/constants.ts'
import { errorMessage, fmtC } from '../lib/format.ts'
import { modeTemp } from '../lib/rooms.ts'

const EDITABLE = ['regular', 'eco', 'turbo'] as const
type EditableMode = (typeof EDITABLE)[number]
type Draft = Record<EditableMode, number | null>

const MIN = 5
const MAX = 30

function hasModeTemps(v: unknown): v is { mode_temps: Snapshot['mode_temps'] } {
  return typeof v === 'object' && v !== null && 'mode_temps' in v
}

/** The setpoint of each mode, editable without changing the room's mode: the thermostat only
 *  keeps one setpoint per mode and only moves the active one, so the server visits each mode
 *  briefly and comes back. */
export function ModeTemps({ room, S }: { room: Room; S: Snapshot }) {
  const { api, toast, setS, rereadSoon } = useApp()
  const { staleData, blockedToast } = useRoomControlGuard()
  const current = (): Draft => ({
    regular: modeTemp(room, 'regular', S),
    eco: modeTemp(room, 'eco', S),
    turbo: modeTemp(room, 'turbo', S),
  })
  const [draft, setDraft] = useState<Draft | null>(null)
  const [busy, setBusy] = useState<'save' | 'read' | null>(null)
  const values = draft ?? current()
  const changed = EDITABLE.filter((m) => values[m] !== null && values[m] !== current()[m])
  const unknown = EDITABLE.filter((m) => current()[m] === null)
  const readOnly = !!S.read_only
  const disabled = busy !== null || readOnly || !!room.vacation

  const step = (mode: EditableMode, delta: number) => {
    if (staleData) {
      blockedToast()
      return
    }
    const base = values[mode] ?? 20
    const next = Math.min(MAX, Math.max(MIN, Math.round((base + delta) * 2) / 2))
    setDraft({ ...values, [mode]: next })
  }

  const send = async (body: Partial<Record<EditableMode, number>> & { read?: boolean }, kind: 'save' | 'read') => {
    setBusy(kind)
    try {
      const r = await api(`/api/rooms/${room.id}/modes`, { body })
      if (hasModeTemps(r)) setS((prev) => (prev ? { ...prev, mode_temps: r.mode_temps } : prev))
      setDraft(null)
      rereadSoon()
      toast(kind === 'save' ? 'Temperaturas guardadas en el termostato' : 'Temperaturas leídas del termostato')
    } catch (e) {
      toast('No se aplicó. ' + errorMessage(e), 6000)
    } finally {
      setBusy(null)
    }
  }

  return (
    <>
      <h2 className="group-title">Temperatura de cada modo</h2>
      <div className="card section modetemps">
        {EDITABLE.map((m) => (
          <div className="mt-row" key={m}>
            <span className={`sw slot-${m}`} aria-hidden="true" />
            <span className="mt-name">{MODE_LABEL[m]}</span>
            <button
              type="button"
              className="round mt-btn"
              disabled={disabled || values[m] === null || values[m]! <= MIN}
              aria-label={`Bajar ${MODE_LABEL[m]} medio grado`}
              onClick={() => step(m, -0.5)}
            >
              −
            </button>
            <b className={`mt-val ${values[m] !== current()[m] ? 'changed' : ''}`}>
              {values[m] === null ? 'sin dato' : fmtC(values[m]) + '°'}
            </b>
            <button
              type="button"
              className="round mt-btn"
              disabled={disabled || values[m]! >= MAX}
              aria-label={`Subir ${MODE_LABEL[m]} medio grado`}
              onClick={() => step(m, 0.5)}
            >
              +
            </button>
          </div>
        ))}
        <p className="muted small">
          {room.vacation
            ? 'En vacaciones no se pueden cambiar.'
            : busy === 'save'
              ? 'Cambiando. El termostato pasa un momento por cada modo y vuelve al que tenía.'
              : busy === 'read'
                ? 'Leyendo. El termostato pasa un momento por cada modo y vuelve al que tenía.'
                : 'Se cambian sin tocar el modo ni el horario de la habitación.'}
        </p>
        <div className="actions">
          {changed.length ? (
            <>
              <button type="button" className="btn" disabled={busy !== null} onClick={() => setDraft(null)}>
                Descartar
              </button>
              <button
                type="button"
                className="btn primary"
                disabled={disabled}
                onClick={() => void send(Object.fromEntries(changed.map((m) => [m, values[m]])), 'save')}
              >
                {busy === 'save' ? 'Guardando…' : 'Guardar'}
              </button>
            </>
          ) : unknown.length ? (
            <button type="button" className="btn" disabled={disabled} onClick={() => void send({ read: true }, 'read')}>
              {busy === 'read' ? 'Leyendo…' : 'Leer del termostato'}
            </button>
          ) : null}
        </div>
      </div>
    </>
  )
}
