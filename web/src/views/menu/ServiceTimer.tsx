import { Fragment, useCallback, useMemo, useState } from 'react'
import { DAYS } from '../../lib/constants'

type SlotMode = 'on' | 'off'

function buildDays(detail: { timer?: { value?: number }[] }) {
  return Array.from({ length: 7 }, (_, day) =>
    Array.from({ length: 48 }, (_, s) => {
      const w = (detail.timer || [])[day * 3 + Math.floor(s / 16)]?.value || 0
      return ((w >>> ((s % 16) * 2)) & 3 ? 'on' : 'off') as SlotMode
    }),
  )
}

export function ServiceTimer({
  detail,
  onSetDay,
  onCopy,
  menuBusy,
}: {
  detail: { timer?: { value?: number }[] }
  onSetDay: (day: number, slots: SlotMode[]) => void
  onCopy: (day: number, mask: number) => void
  menuBusy: boolean
}) {
  const [stDay, setStDay] = useState(() => (new Date().getDay() + 6) % 7)
  const [stDays, setStDays] = useState<SlotMode[][]>(() => buildDays(detail))

  const hours = useMemo(
    () =>
      Array.from({ length: 12 }, (_, i) => (
        <div className="h" key={i}>
          {String(i * 2).padStart(2, '0')}
        </div>
      )),
    [],
  )

  const toggleSlot = useCallback((d: number, i: number) => {
    setStDays((prev) => {
      const next = prev.map((row) => [...row])
      next[d][i] = next[d][i] === 'on' ? 'off' : 'on'
      return next
    })
    setStDay(d)
  }, [])

  const handleCopy = useCallback(
    (mask: number) => {
      if (
        !confirm(
          `¿Copiar el ${DAYS[stDay].toLowerCase()} a los días elegidos?`,
        )
      )
        return
      onCopy(stDay, mask)
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
        {hours}
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
                onClick={() => toggleSlot(day, i)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') toggleSlot(day, i)
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
          onClick={() => onSetDay(stDay, stDays[stDay])}
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
