import { useEffect, useState } from 'react'
import { Icon } from '../components/Icon.tsx'
import { useApp } from '../context/AppContext.tsx'
import { useNow } from '../hooks/useNow.ts'
import { roomById, roomTitle } from '../lib/rooms.ts'

const LOG_TEXT: [RegExp, string][] = [
  [/^Reinicio$/i, 'El equipo se reinició'],
  [/CAN_Bus está conectado/i, 'Conectado con la centralita'],
  [/CAN_Bus.*(desconectado|perdido|error)/i, 'Se perdió la conexión con la centralita'],
]

function friendlyLog(msg: string) {
  for (const [re, t] of LOG_TEXT) if (re.test(msg)) return t
  return msg
}

/** Which channel asked the server to act; the device's own log has none. */
type Channel = 'web' | 'ha' | 'auto' | 'server'

const CHANNEL_LABEL: Record<Channel, string | null> = {
  web: 'web',
  ha: 'Home Assistant',
  auto: 'automático',
  server: null,
}

type Entry = {
  ts: number
  kind: 'device' | 'server'
  channel: Channel | null
  room: number | null
  text: string
  raw: string
  warn: boolean
  /** a read, not a change: only worth seeing when looking at the server's doings */
  quiet: boolean
  /** identical server rows on the same day fold into one, spanning from `first` to `ts` */
  count: number
  first: number
}

type Filter = 'all' | 'device' | 'server'

function isLogLine(v: unknown): v is { dt?: string; msg: string } {
  return typeof v === 'object' && v !== null && 'msg' in v && typeof (v as { msg: unknown }).msg === 'string'
}

function isEvent(v: unknown): v is { ts: number; source: string; room: number | null; text: string } {
  return (
    typeof v === 'object' &&
    v !== null &&
    typeof (v as { ts: unknown }).ts === 'number' &&
    typeof (v as { text: unknown }).text === 'string'
  )
}

function isChannel(s: string): s is Channel {
  return s in CHANNEL_LABEL
}

const WARN = /perdió|no responde|falló/

/** Server messages worth reading as a feed: no file names, no protocol words. */
function serverText(raw: string): string {
  if (/^El termostato no responde/.test(raw)) return 'El termostato no responde'
  return raw
}

const pad = (n: number) => String(n).padStart(2, '0')
const dayKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
const DAY = 86400

function dayLabel(key: string, todayKey: string, yesterdayKey: string): string {
  if (key === todayKey) return 'Hoy'
  if (key === yesterdayKey) return 'Ayer'
  const d = new Date(key + 'T12:00')
  const sameYear = d.getFullYear() === Number(key.slice(0, 4)) && key.slice(0, 4) === todayKey.slice(0, 4)
  return d.toLocaleDateString('es-ES', {
    weekday: 'short',
    day: 'numeric',
    month: 'long',
    ...(sameYear ? {} : { year: 'numeric' }),
  })
}

function gapLabel(days: number): string {
  if (days >= 60) return `${Math.floor(days / 30)} meses sin avisos`
  if (days >= 45) return 'Un mes sin avisos'
  if (days >= 21) return `${Math.floor(days / 7)} semanas sin avisos`
  return ''
}

export function InfoView() {
  const { S, api } = useApp()
  const [entries, setEntries] = useState<Entry[] | null>(null)
  const [failed, setFailed] = useState<string[]>([])
  const [filter, setFilter] = useState<Filter>('all')
  const nowMs = useNow()

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const bad: string[] = []
      const [log, events] = await Promise.all([
        api('/api/log').catch(() => {
          bad.push('los avisos del termostato')
          return []
        }),
        api('/api/events?limit=400').catch(() => {
          bad.push('el registro del servidor')
          return []
        }),
      ])
      if (cancelled) return
      const out: Entry[] = []
      // the device's own log: "2026-08-08 - 13:13" in the device's local time
      let last: Entry | null = null
      for (const l of Array.isArray(log) ? log.filter(isLogLine) : []) {
        const m = /^(\d{4}-\d{2}-\d{2}) - (\d{2}:\d{2})/.exec(l.dt || '')
        if (!m) continue
        const ts: number = new Date(`${m[1]}T${m[2]}:00`).getTime() / 1000
        const text = friendlyLog(l.msg)
        if (last && last.text === text && Math.abs(last.ts - ts) <= 90) continue
        last = { ts, kind: 'device', channel: null, room: null, text, raw: l.msg, warn: WARN.test(text), quiet: false, count: 1, first: ts }
        out.push(last)
      }
      for (const e of Array.isArray(events) ? events.filter(isEvent) : []) {
        const text = serverText(e.text)
        out.push({
          ts: e.ts,
          kind: 'server',
          channel: isChannel(e.source) ? e.source : 'server',
          room: e.room ?? null,
          text,
          raw: e.text,
          warn: WARN.test(text),
          quiet: /leídas/.test(text),
          count: 1,
          first: e.ts,
        })
      }
      out.sort((a, b) => b.ts - a.ts)
      // the same server row repeated on one day (restarts during a deploy, a crash loop) reads as
      // one row with a count and the span, even with other rows in between
      const folded: Entry[] = []
      const seen = new Map<string, Entry>()
      for (const e of out) {
        const key = e.kind === 'server' ? `${dayKey(new Date(e.ts * 1000))}|${e.text}|${e.room}` : null
        const prev = key ? seen.get(key) : undefined
        if (prev) {
          prev.count += 1
          prev.first = e.ts
          continue
        }
        const copy = { ...e }
        folded.push(copy)
        if (key) seen.set(key, copy)
      }
      setEntries(folded)
      setFailed(bad)
    })()
    return () => {
      cancelled = true
    }
  }, [api])

  if (!S) return null

  const all = entries ?? []
  const counts = {
    all: all.filter((e) => !e.quiet).length,
    device: all.filter((e) => e.kind === 'device').length,
    server: all.filter((e) => e.kind === 'server').length,
  }
  const shown = all.filter((e) =>
    filter === 'all' ? !e.quiet : filter === 'device' ? e.kind === 'device' : e.kind === 'server',
  )

  const todayKey = dayKey(new Date(nowMs))
  const yesterdayKey = dayKey(new Date(nowMs - DAY * 1000))
  const days: { key: string; gap: string; items: Entry[] }[] = []
  for (const e of shown) {
    const key = dayKey(new Date(e.ts * 1000))
    const d = days[days.length - 1]
    if (d && d.key === key) d.items.push(e)
    else {
      const prevTs = d?.items[d.items.length - 1]?.ts
      const gap = prevTs ? gapLabel(Math.round((prevTs - e.ts) / DAY)) : ''
      days.push({ key, gap, items: [e] })
    }
  }

  const device = all.filter((e) => e.kind === 'device')
  const reboots = device.filter((e) => /reinició/.test(e.text)).reduce((n, e) => n + e.count, 0)
  const oldest = device[device.length - 1]
  const lastLink = device.find((e) => /^Conectado/.test(e.text))
  const fact = [
    reboots && oldest
      ? `${reboots} ${reboots === 1 ? 'reinicio' : 'reinicios'} desde ${new Date(oldest.ts * 1000).toLocaleDateString('es-ES', { month: 'long', year: 'numeric' })}`
      : null,
    lastLink
      ? `última conexión con la centralita el ${new Date(lastLink.ts * 1000).toLocaleDateString('es-ES', { day: 'numeric', month: 'short' })}`
      : null,
  ]
    .filter((s): s is string => !!s)
    .join(' · ')

  const roomPrefix = (e: Entry) => {
    const r = e.room === null ? null : roomById(S, e.room)
    return r ? `${roomTitle(r)}: ` : ''
  }

  return (
    <>
      <div className="page-head">
        <h3>Avisos</h3>
      </div>
      {fact ? <p className="lead tl-fact">{fact}</p> : null}
      {failed.length ? (
        <div className="banner">
          <Icon name="warn" className="mini" /> No pude leer {failed.join(' ni ')}.
        </div>
      ) : null}
      <div className="tl-filterbar">
      <div className="seg" id="logFilter" role="tablist" aria-label="Origen">
        {(
          [
            ['all', 'Todo'],
            ['device', 'Termostato'],
            ['server', 'Servidor'],
          ] as [Filter, string][]
        ).map(([k, label]) => (
          <button
            key={k}
            type="button"
            role="tab"
            aria-selected={filter === k}
            className={filter === k ? 'active' : ''}
            onClick={() => setFilter(k)}
          >
            {label}
            {entries ? <span className="tl-count">{counts[k]}</span> : null}
          </button>
        ))}
      </div>
      </div>
      {entries === null ? (
        <div className="empty">
          <span className="spin" /> Leyendo los avisos…
        </div>
      ) : shown.length ? (
        <div className="timeline">
          {days.map((d) => (
            <div className="tl-day" key={d.key}>
              {d.gap ? <div className="tl-gap">{d.gap}</div> : null}
              <div className="tl-date">{dayLabel(d.key, todayKey, yesterdayKey)}</div>
              {d.items.map((e, k) => (
                <div className={`tl-item ${e.warn ? 'warn' : ''} ${e.kind}`} key={`${e.ts}-${k}`}>
                  <span className="tl-time">
                    {new Date(e.ts * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })}
                  </span>
                  <span className="tl-ic" aria-label={e.kind === 'device' ? 'Termostato' : 'Servidor'}>
                    <Icon name={e.warn ? 'warn' : e.kind === 'device' ? 'thermo' : 'chip'} className="mini" />
                  </span>
                  <span className="tl-text" title={e.raw}>
                    {roomPrefix(e)}
                    {e.text}
                    {e.channel && CHANNEL_LABEL[e.channel] && !e.text.startsWith('Copia automática') ? (
                      <>
                        {' '}
                        <span className="badge">{CHANNEL_LABEL[e.channel]}</span>
                      </>
                    ) : null}
                    {e.count > 1 ? (
                      <>
                        {' '}
                        <span className="tl-count">
                          ×{e.count}
                          {e.first !== e.ts
                            ? ` · ${new Date(e.first * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })}–${new Date(e.ts * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })}`
                            : ''}
                        </span>
                      </>
                    ) : null}
                  </span>
                </div>
              ))}
            </div>
          ))}
        </div>
      ) : (
        <div className="empty">
          {filter === 'device'
            ? 'El termostato no tiene avisos. Aquí salen sus reinicios y la conexión con la centralita.'
            : filter === 'server'
              ? 'El servidor todavía no ha anotado nada. Aquí salen los cambios que envía al termostato (desde la web o Home Assistant), las copias, y cuándo el termostato deja de responder.'
              : 'Nada que mostrar todavía.'}
        </div>
      )}
    </>
  )
}
