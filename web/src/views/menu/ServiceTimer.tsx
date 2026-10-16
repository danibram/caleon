import { Fragment, useCallback, useState } from 'react'
import { DAYS } from '../../lib/constants'

type SlotMode = 'on' | 'off'

const HOURS = Array.from({ length: 12 }, (_, i) => (
  <div className="h" key={i}>
    {String(i * 2).padStart(2, '0')}
  </div>
))

/** Three 16-bit words per day, two bits per half hour. */
function buildDays(words: number[]) {
  return Array.from({ length: 7 }, (_, day) =>
    Array.from({ length: 48 }, (_, s): SlotMode => {
      const w = words[day * 3 + Math.floor(s / 16)] || 0
      return (w >>> ((s % 16) * 2)) & 3 ? 'on' : 'off'
    }),
  )
}

export function ServiceTimer({
  words,
  onSetDay,
  onCopy,
  menuBusy,
}: {
  words: number[]
  onSetDay: (args: { day: number; slots: SlotMode[] }) => void
  onCopy: (args: { day: number; mask: number }) => void
  menuBusy: boolean
}) {
  const [stDay, setStDay] = useState(() => (new Date().getDay() + 6) % 7)
  const [stDays, setStDays] = useState<SlotMode[][]>(() => buildDays(words))

  const toggleSlot = useCallback(({ day, slot }: { day: number; slot: number }) => {
    setStDays((prev) => {
      const next = prev.map((row) => [...row])
      next[day][slot] = next[day][slot] === 'on' ? 'off' : 'on'
      return next
    })
    setStDay(day)
  }, [])

  const handleCopy = useCallback(
    (mask: number) => {
      if (
        !confirm(
          `¿Copiar el ${DAYS[stDay].toLowerCase()} a los días elegidos?`,
        )
      )
        return
      onCopy({ day: stDay, mask })
    },
    [onCopy, stDay],
  )

  return (
    <>
      <p className="muted small">
        Pulsa las franjas para alternar encendido/apagado y guarda el día.
      </p>
      <div className="week" id="sWeek">
        <div />
        {HOURS}
        {stDays.map((slots, day) => (
          <Fragment key={day}>
            <div
              className="d"
              data-day={day}
              style={{
                textDecoration: day === stDay ? 'underline' : undefined,
              }}
              onClick={() => setStDay(day)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') setStDay(day)
              }}
              role="button"
              tabIndex={0}
            >
              {DAYS[day].slice(0, 3)}
            </div>
            {slots.map((m, i) => (
              <div
                key={`${day}-${i}`}
                className={`s slot-${m}`}
                data-d={day}
                data-i={i}
                onClick={() => toggleSlot({ day, slot: i })}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') toggleSlot({ day, slot: i })
                }}
                role="button"
                tabIndex={0}
              />
            ))}
          </Fragment>
        ))}
      </div>
      <div className="actions" style={{ marginTop: 12 }}>
        <button
          type="button"
          className="btn primary"
          id="sSave"
          disabled={menuBusy}
          onClick={() => onSetDay({ day: stDay, slots: stDays[stDay] })}
        >
          Guardar {DAYS[stDay].toLowerCase()}
        </button>
        <button
          type="button"
          className="btn"
          disabled={menuBusy}
          onClick={() => handleCopy(31)}
        >
          Copiar a lunes-viernes
        </button>
        <button
          type="button"
          className="btn"
          disabled={menuBusy}
          onClick={() => handleCopy(127)}
        >
          Copiar a lunes-domingo
        </button>
      </div>
    </>
  )
}
