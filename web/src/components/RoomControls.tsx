import { MODES, SYSTEM } from '../lib/constants.ts'
import type { Room, Snapshot } from '../context/AppContext.tsx'
import { es, fmtC } from '../lib/format.ts'
import { modeOf, modeTemp } from '../lib/rooms.ts'
import { useRoomChange, useRoomControlGuard } from '../hooks/useRoomChange.ts'

type Compact = boolean | 'modes'

interface RoomControlsProps {
  room: Room
  S: Snapshot | null
  compact?: Compact
}

export function RoomControls({ room, S, compact = false }: RoomControlsProps) {
  const roomChange = useRoomChange()
  const { staleData, roomBusy, blockedToast } = useRoomControlGuard()
  const mode = modeOf(room)
  const canAdjust = room.id !== SYSTEM && mode !== 'off' && !room.vacation
  const idx = MODES.findIndex(([k]) => k === mode)
  const busy = roomBusy.has(Number(room.id))

  const act = (body: { tdelta?: number; mode?: string }) => {
    if (staleData) {
      blockedToast()
      return
    }
    if (busy) return
    void roomChange(room.id, body)
  }

  const showSetpoint = room.id !== SYSTEM && compact !== 'modes'

  return (
    <>
      {showSetpoint ? (
        <div className="setpoint">
          <button
            type="button"
            className="round"
            disabled={!canAdjust}
            aria-label="Bajar consigna medio grado"
            onClick={() => act({ tdelta: -1 })}
          >
            −
          </button>
          <div>
            <div className="label">{mode === 'off' ? 'Apagada' : 'Consigna'}</div>
            <div className="value" data-tick={`set${room.id}`}>
              {mode === 'off' ? '—' : es((room.tset || '—').replace('.', ','))}
            </div>
          </div>
          <button
            type="button"
            className="round"
            disabled={!canAdjust}
            aria-label="Subir consigna medio grado"
            onClick={() => act({ tdelta: 1 })}
          >
            +
          </button>
        </div>
      ) : null}
      <div
        className={`modes ${idx < 0 ? 'no-active' : ''}`}
        role="group"
        aria-label="Modo"
        data-mid={room.id}
        data-i={idx}
        data-am={idx < 0 ? '' : mode}
      >
        <span
          className="mode-ind"
          style={{ transform: `translateX(calc(${Math.max(idx, 0)} * (100% + 5px)))` }}
        />
        {MODES.map(([k, label]) => (
          <button
            key={k}
            type="button"
            data-act="mode"
            data-v={k}
            data-mode={k}
            className={k === mode ? 'active' : ''}
            aria-pressed={k === mode}
            disabled={!!room.vacation}
            onClick={() => act({ mode: k })}
          >
            {label}
            {room.id !== SYSTEM ? (
              <small>
                {k === 'off'
                  ? '\u00a0'
                  : modeTemp(room, k, S) !== null
                    ? fmtC(modeTemp(room, k, S)) + '°'
                    : '–°'}
              </small>
            ) : null}
          </button>
        ))}
      </div>
    </>
  )
}
