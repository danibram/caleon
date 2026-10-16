import { useEffect, useState } from 'react'
import { Link } from 'react-router'
import { Icon } from '../components/Icon.tsx'
import { useApp } from '../context/AppContext.tsx'
import { fmtFirmware } from '../lib/format.ts'
import { roomTitle, zoneRooms } from '../lib/rooms.ts'

const LOG_TEXT: [RegExp, string][] = [
  [/^Reinicio$/i, 'El equipo se reinició'],
  [/CAN_Bus está conectado/i, 'Conectado con la centralita'],
  [/CAN_Bus.*(desconectado|perdido|error)/i, 'Se perdió la conexión con la centralita'],
]

function friendlyLog(msg: string) {
  for (const [re, t] of LOG_TEXT) if (re.test(msg)) return t
  return msg
}

type LogLine = { dt?: string; msg: string }

type LogItem = { date: string; time: string; text: string; raw: string }

export function InfoView() {
  const { S, api } = useApp()
  const [items, setItems] = useState<LogItem[]>([])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      let log: LogLine[] = []
      try {
        log = await api<LogLine[]>('/api/log')
      } catch {
        /* empty */
      }
      if (cancelled) return
      const parsed: LogItem[] = []
      for (const l of log) {
        const m = /^(\d{4}-\d{2}-\d{2}) - (\d{2}:\d{2})/.exec(l.dt || '')
        const date = m ? m[1]! : ''
        const time = m ? m[2]! : l.dt || ''
        const text = friendlyLog(l.msg)
        const prev = parsed[parsed.length - 1]
        if (
          prev &&
          prev.date === date &&
          prev.text === text &&
          Math.abs(Number(prev.time.replace(':', '')) - Number(time.replace(':', ''))) <= 1
        )
          continue
        parsed.push({ date, time, text, raw: l.msg })
      }
      setItems(parsed)
    })()
    return () => {
      cancelled = true
    }
  }, [api])

  if (!S) return null

  const days: { date: string; items: LogItem[] }[] = []
  for (const it of items) {
    const d = days[days.length - 1]
    if (d && d.date === it.date) d.items.push(it)
    else days.push({ date: it.date, items: [it] })
  }

  const fmtDate = (d: string) =>
    d
      ? new Date(d + 'T12:00').toLocaleDateString('es-ES', {
          weekday: 'short',
          day: 'numeric',
          month: 'long',
          year: 'numeric',
        })
      : ''

  const zones = Object.entries(zoneRooms(S))
    .map(([z, r]) => `zona ${z}: ${roomTitle(r)}`)
    .join(' · ')

  const reboots = items.filter((i) => /reinició/.test(i.text)).length

  return (
    <>
      <Link className="back" to="/settings">
        <Icon name="back" className="mini" /> Ajustes
      </Link>
      <div className="page-head">
        <h3>Avisos</h3>
      </div>
      <p className="lead">
        {items.length
          ? `${reboots ? `Desde ${fmtDate(items[items.length - 1]!.date)} el equipo se ha reiniciado ${reboots} ${reboots === 1 ? 'vez' : 'veces'}, normalmente por cortes de luz, y siempre ha vuelto a conectar con la centralita. ` : ''}Último aviso: ${fmtDate(items[0]!.date)}.`
          : 'El equipo no tiene avisos.'}
      </p>
      <div className="timeline">
        {days.map((d) => (
          <div className="tl-day" key={d.date}>
            <div className="tl-date">{fmtDate(d.date)}</div>
            {d.items.map((i) => (
              <div className="tl-item" key={i.date + i.time + i.raw}>
                <span className="tl-time">{i.time}</span>
                <span
                  className={`tl-dot ${/reinició|perdió/.test(i.text) ? 'warn' : ''}`}
                />
                <span title={i.raw}>{i.text}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
      <h2 className="group-title">Equipo</h2>
      <div className="card">
        <div className="kv">
          <span>Termostato</span>
          <b>{S.device.split('.')[0]}</b>
        </div>
        <div className="kv">
          <span>Firmware</span>
          <b>{fmtFirmware(S.firmware)}</b>
        </div>
        {zones ? (
          <div className="kv">
            <span>Zonas</span>
            <b>{zones}</b>
          </div>
        ) : null}
        <details className="adv" style={{ marginTop: 10 }}>
          <summary>Detalles técnicos</summary>
          <div className="small muted" style={{ marginTop: 10 }}>
            Nabto {S.nabto || '—'} · funciones:{' '}
            {Object.entries(S.features || {})
              .filter(([, v]) => v)
              .map(([k]) => k)
              .join(', ')}
          </div>
        </details>
      </div>
    </>
  )
}
