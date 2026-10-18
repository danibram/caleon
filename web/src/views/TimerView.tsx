import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import { Link, useBlocker, useNavigate, useParams } from 'react-router'
import { Banner } from '../components/Banner.tsx'
import { Icon } from '../components/Icon.tsx'
import { useApp } from '../context/AppContext.tsx'
import { DAYS, MODE_LABEL, SYSTEM } from '../lib/constants.ts'
import { errorMessage } from '../lib/format.ts'
import { hhmm, modeWithTemp, nowSlotIdx, segments, todayIdx } from '../lib/schedule.ts'
import { roomById, roomTitle } from '../lib/rooms.ts'

type TimerWeek = { days: string[][]; modes: string[] }

const reduceMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches

const COPY_OPTS: [string, string][] = [
  ['next', 'Día siguiente'],
  ['31', 'Lunes a viernes'],
  ['96', 'Fin de semana'],
  ['127', 'Toda la semana'],
]

export function TimerView() {
  const { id: idParam } = useParams()
  const id = Number(idParam)
  const navigate = useNavigate()
  const { S, api, toast, ask, rereadSoon } = useApp()

  const r = roomById(S, id)
  const isSys = id === SYSTEM
  const today = todayIdx()

  const [type, setType] = useState(() => (isSys ? 2 : r?.season === 'cooling' ? 1 : 0))
  const [week, setWeekState] = useState<TimerWeek | null>(null)
  const [original, setOriginal] = useState<string[][] | null>(null)
  const [brush, setBrush] = useState<string | null>(null)
  const [dirty, setDirty] = useState(() => new Set<number>())
  const [day, setDay] = useState(today)
  const [saving, setSaving] = useState(false)
  const [copyOpen, setCopyOpen] = useState(false)
  const [loadErr, setLoadErr] = useState<string | null>(null)
  const draftAt = useRef(0)

  const painting = useRef(false)
  const hold = useRef<{ timer: ReturnType<typeof setTimeout>; i: number; x: number; y: number } | null>(null)
  const lastI = useRef<number | null>(null)
  const pending = useRef(false)
  const dayEditRef = useRef<HTMLDivElement>(null)
  // latest days, so several cells painted before React re-renders build on each other
  const daysRef = useRef<string[][] | null>(null)

  const setWeek = useCallback((w: TimerWeek | null) => {
    daysRef.current = w?.days ?? null
    setWeekState(w)
  }, [])

  const setDays = (days: string[][]) => {
    daysRef.current = days
    setWeekState((w) => (w ? { ...w, days } : w))
  }

  useEffect(() => {
    if (!idParam) return
    let cancelled = false
    ;(async () => {
      try {
        const w = await api<TimerWeek>(`/api/rooms/${id}/timer?type=${type}`)
        if (cancelled) return
        setWeek(w)
        setOriginal(structuredClone(w.days))
        setDirty(new Set())
        setBrush((prev) => (prev && w.modes.includes(prev) ? prev : w.modes[1]))
        setLoadErr(null)
      } catch (e) {
        if (cancelled) return
        setLoadErr(errorMessage(e))
        setWeek(null)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [api, id, idParam, type, setWeek])

  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirty.size > 0 &&
      !saving &&
      currentLocation.pathname !== nextLocation.pathname,
  )

  useEffect(() => {
    if (S && !roomById(S, id)) navigate('/', { replace: true })
  }, [S, id, navigate])

  useEffect(() => {
    if (blocker.state !== 'blocked') return
    let cancelled = false
    ;(async () => {
      const ok = await ask({
        title: '¿Descartar los cambios?',
        text: 'Hay días pintados sin guardar en el termostato.',
        input: false,
        ok: 'Descartar',
      })
      if (cancelled) return
      if (ok) blocker.proceed()
      else blocker.reset()
    })()
    return () => {
      cancelled = true
    }
  }, [blocker, ask])

  if (!S || !r) return null

  const syncDirtyForDay = ({ d, days, orig }: { d: number; days: string[][]; orig: string[][] }) => {
    const changed = days[d].some((m, k) => m !== orig[d][k])
    setDirty((prev) => {
      const next = new Set(prev)
      if (changed) next.add(d)
      else next.delete(d)
      return next
    })
  }

  const cellIndex = (el: Element | null | undefined): number | null => {
    const cell = el?.closest('.cell')
    return cell instanceof HTMLElement ? Number(cell.dataset.i) : null
  }

  const paintCells = (cells: number[]) => {
    const days = daysRef.current
    if (!days || !original || !brush) return
    const todo = cells.filter((i) => days[day]![i] !== brush)
    if (!todo.length) return
    if (!dirty.size && !pending.current) draftAt.current = Date.now()
    const row = days[day]!.slice()
    for (const i of todo) row[i] = brush
    const nextDays = days.map((r, d) => (d === day ? row : r))
    setDays(nextDays)
    syncDirtyForDay({ d: day, days: nextDays, orig: original })
    pending.current = true
  }

  const paintAt = (i: number) => {
    const cells = [i]
    if (lastI.current !== null && (i - lastI.current) % 2 === 0 && Math.abs(i - lastI.current) > 2) {
      const [a, b] = i > lastI.current ? [lastI.current, i] : [i, lastI.current]
      for (let k = a + 2; k < b; k += 2) cells.push(k)
    }
    lastI.current = i
    paintCells(cells)
  }

  // On touch a swipe scrolls the page; painting a run needs a short hold first, so a finger can
  // move through the grid without changing anything. A tap still paints one half hour.
  const HOLD_MS = 220

  const startPainting = (pointerId: number, i: number) => {
    if (!dayEditRef.current) return
    painting.current = true
    try {
      dayEditRef.current.setPointerCapture(pointerId)
    } catch {
      // the pointer is gone already (a very short touch); painting still works for this event
    }
    paintAt(i)
    if (navigator.vibrate) navigator.vibrate(8)
  }

  const cancelHold = () => {
    if (hold.current) clearTimeout(hold.current.timer)
    hold.current = null
  }

  const onPointerDown = (ev: ReactPointerEvent) => {
    const i = ev.target instanceof Element ? cellIndex(ev.target) : null
    if (i === null || !dayEditRef.current) return
    if (ev.pointerType === 'mouse') {
      startPainting(ev.pointerId, i)
      ev.preventDefault()
      return
    }
    cancelHold()
    const { pointerId, clientX, clientY } = ev
    hold.current = {
      i,
      x: clientX,
      y: clientY,
      timer: setTimeout(() => {
        hold.current = null
        startPainting(pointerId, i)
      }, HOLD_MS),
    }
  }

  const onPointerMove = (ev: ReactPointerEvent) => {
    if (hold.current && Math.hypot(ev.clientX - hold.current.x, ev.clientY - hold.current.y) > 8) {
      cancelHold() // the finger is scrolling
      return
    }
    if (!painting.current) return
    const i = cellIndex(document.elementFromPoint(ev.clientX, ev.clientY))
    if (i !== null) paintAt(i)
  }

  const endPaint = (ev?: ReactPointerEvent) => {
    if (hold.current && ev?.type === 'pointerup') {
      const { i } = hold.current
      cancelHold()
      paintAt(i) // a tap
      lastI.current = null
    }
    cancelHold()
    painting.current = false
    lastI.current = null
    if (pending.current && daysRef.current && original) {
      syncDirtyForDay({ d: day, days: daysRef.current, orig: original })
      pending.current = false
    }
  }

  const save = async () => {
    const local = daysRef.current
    if (!week || !local || !original) return
    setSaving(true)
    try {
      let w = week
      let orig = original
      for (const d of [...dirty].sort((a, b) => a - b)) {
        w = await api<TimerWeek>(`/api/rooms/${id}/timer/${d}?type=${type}`, {
          method: 'PUT',
          body: { slots: local[d] },
        })
        const saved = w.days[d]!.slice()
        orig = orig.map((row, k) => (k === d ? saved : row))
        setOriginal(orig)
        setDirty((prev) => {
          const next = new Set(prev)
          next.delete(d)
          return next
        })
      }
      setWeek(w)
      toast('Horario guardado en el termostato')
      rereadSoon()
    } catch (e) {
      toast('No se pudo guardar: ' + errorMessage(e), 6000)
    }
    setSaving(false)
  }

  const copy = async (what: string) => {
    if (!week) return
    if (dirty.size) {
      toast('Guarda o descarta los cambios antes de copiar')
      return
    }
    const mask = what === 'next' ? 1 << ((day + 1) % 7) : Number(what)
    if (
      !confirm(
        `¿Copiar el horario del ${DAYS[day]!.toLowerCase()} a los días elegidos? Se sobrescriben.`,
      )
    )
      return
    try {
      const w = await api<TimerWeek>(`/api/rooms/${id}/timer/${day}/copy?type=${type}`, {
        body: { mask },
      })
      setWeek(w)
      setOriginal(structuredClone(w.days))
      setCopyOpen(false)
      toast('Horario copiado')
      rereadSoon()
    } catch (e) {
      toast('No se pudo copiar: ' + errorMessage(e), 6000)
    }
  }

  const reset = () => {
    if (!original || !week) return
    setDays(original.map((row) => row.slice()))
    setDirty(new Set())
  }

  if (!week && !loadErr) {
    return (
      <div className="empty">
        <span className="spin" /> Leyendo horarios del termostato…
      </div>
    )
  }

  if (loadErr) {
    return <div className="banner">No pude leer los horarios: {loadErr}</div>
  }

  if (!week || !original || !brush) return null

  const slots = week.days[day]!
  const segs = segments(slots)
  const ns = nowSlotIdx()
  const showTypeSeg =
    !isSys && (S.features?.season_menu || r.season === 'cooling')
  const coarse = matchMedia('(any-pointer: coarse)').matches

  return (
    <>
      <Banner />
      <Link className="back" to={`/room/${id}`}>
        <Icon name="back" className="mini" /> {roomTitle(r)}
      </Link>
      <div className="page-head">
        <h3>{isSys ? 'Agua caliente' : 'Horarios'}</h3>
        {showTypeSeg ? (
          <div className="seg" id="tType">
            <button
              type="button"
              data-t="0"
              className={type === 0 ? 'active' : ''}
              onClick={() => {
                if (dirty.size && !confirm('Hay cambios sin guardar. ¿Descartarlos?')) return
                setType(0)
              }}
            >
              Calefacción
            </button>
            <button
              type="button"
              data-t="1"
              className={type === 1 ? 'active' : ''}
              onClick={() => {
                if (dirty.size && !confirm('Hay cambios sin guardar. ¿Descartarlos?')) return
                setType(1)
              }}
            >
              Refrigeración
            </button>
          </div>
        ) : null}
      </div>
      <div className="sched">
        <div className="sched-side">
          <div className="weekmini" aria-label="Semana completa">
            {week.days.map((sl, d) => (
              <button
                key={d}
                type="button"
                className={`wrow ${d === day ? 'active' : ''} ${d === today ? 'today' : ''}`}
                aria-pressed={d === day}
                aria-label={DAYS[d]}
                onClick={() => {
                  setDay(d)
                  setCopyOpen(false)
                }}
              >
                <span className="wlabel">
                  {DAYS[d]!.slice(0, 3)}
                  {dirty.has(d) ? <i className="dot" /> : null}
                </span>
                <span className="wbar">
                  {segments(sl).map((g) => (
                    <i
                      key={g.start}
                      className={`slot-${g.mode}`}
                      style={{ flex: g.end - g.start }}
                    />
                  ))}
                  {d === today ? (
                    <b className="now" style={{ left: `${((ns + 0.5) / 48) * 100}%` }} />
                  ) : null}
                </span>
              </button>
            ))}
            <div className="waxis">
              <span>0</span>
              <span>6</span>
              <span>12</span>
              <span>18</span>
              <span>24 h</span>
            </div>
          </div>
        </div>
        <div className="sched-main">
          <div className="paintbar">
            <div className="brushes" role="radiogroup" aria-label="Modo para pintar">
              {week.modes.map((m) => (
                <button
                  key={m}
                  type="button"
                  role="radio"
                  aria-checked={m === brush}
                  className={m === brush ? 'active' : ''}
                  onClick={() => setBrush(m)}
                >
                  <span className={`sw slot-${m}`} />
                  {MODE_LABEL[m] || m}
                </button>
              ))}
            </div>
            <div className="paintslot">
              {dirty.size ? (
                <div className="draftbar show" role="region" aria-label="Cambios sin guardar">
                  <span>
                    {dirty.size === 1 ? '1 día cambiado' : `${dirty.size} días cambiados`}
                  </span>
                  <button type="button" className="btn" onClick={reset}>
                    Descartar
                  </button>
                  <button
                    type="button"
                    className="btn primary"
                    disabled={saving}
                    onClick={() => {
                      if (Date.now() - draftAt.current < 600) return
                      void save()
                    }}
                  >
                    {saving ? 'Guardando…' : 'Guardar'}
                  </button>
                </div>
              ) : (
                <div className="hint spread">
                  <span className="muted small">
                    Elige un modo y{' '}
                    {coarse
                      ? 'toca una media hora para pintarla; mantén pulsado y arrastra para pintar varias. Desliza para moverte por el día.'
                      : 'haz clic o arrastra sobre las medias horas.'}
                  </span>
                  {day === today ? (
                    <button
                      type="button"
                      className="chip"
                      id="toNow"
                      onClick={() =>
                        document
                          .querySelector('.cell.is-now')
                          ?.scrollIntoView({
                            behavior: reduceMotion() ? 'auto' : 'smooth',
                            block: 'center',
                          })
                      }
                    >
                      Ir a ahora
                    </button>
                  ) : null}
                </div>
              )}
            </div>
            <div className="hrow hhead">
              <span />
              <span>:00</span>
              <span>:30</span>
            </div>
          </div>
          <div
            className="dayedit"
            id="dayedit"
            ref={dayEditRef}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endPaint}
            onPointerCancel={endPaint}
            onTouchMove={(ev) => {
              // once a hold started a run, keep the browser from scrolling under the finger
              if (painting.current && ev.cancelable) ev.preventDefault()
            }}
          >
            {Array.from({ length: 24 }, (_, h) => (
              <div className="hrow" key={h}>
                <span className="hl">{String(h).padStart(2, '0')}:00</span>
                {[0, 1].map((k) => {
                  const i = h * 2 + k
                  const m = slots[i]!
                  const startSeg = i === 0 || slots[i - 1] !== m
                  const isNow = day === today && i === ns
                  const changed = original[day]![i] !== m
                  return (
                    <div
                      key={i}
                      className={`cell slot-${m} ${isNow ? 'is-now' : ''} ${changed ? 'changed' : ''}`}
                      data-i={i}
                      role="button"
                      aria-label={`${DAYS[day]} ${hhmm(i)}, ${MODE_LABEL[m] || m}`}
                      title={`${hhmm(i)}–${i === 47 ? '24:00' : hhmm(i + 1)}`}
                    >
                      {isNow ? <em>ahora</em> : startSeg ? <span>{MODE_LABEL[m] || m}</span> : null}
                    </div>
                  )
                })}
              </div>
            ))}
          </div>
        </div>
        <div className="sched-sum">
          <div className="daysum card">
            <div className="spread">
              <b>{DAYS[day]}</b>
              <button type="button" className="link-more" id="copyBtn" onClick={() => setCopyOpen((o) => !o)}>
                Copiar a… <Icon name="chevron" className="mini" />
              </button>
            </div>
            {week.days.every((d) => d.join() === week.days[0]!.join()) ? (
              <div className="same-days">
                <Icon name="check" className="mini" /> Los 7 días tienen el mismo horario
              </div>
            ) : null}
            {copyOpen ? (
              <div className="copyopts">
                {COPY_OPTS.map(([k, l]) => (
                  <button key={k} type="button" className="btn" onClick={() => void copy(k)}>
                    {l}
                  </button>
                ))}
              </div>
            ) : null}
            <ol className="periods">
              {segs.map((g) => (
                <li key={g.start}>
                  <span className={`sw slot-${g.mode}`} />
                  <span className="ptime">
                    {hhmm(g.start)}–{g.end === 48 ? '24:00' : hhmm(g.end)}
                  </span>
                  <span className="pmode">
                    {isSys ? MODE_LABEL[g.mode] || g.mode : modeWithTemp(r, g.mode, S)}
                  </span>
                </li>
              ))}
            </ol>
          </div>
        </div>
      </div>
    </>
  )
}
