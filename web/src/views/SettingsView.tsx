import { roomTitle, zoneRooms } from '../lib/rooms.ts'
import { useCallback, useEffect, useState } from 'react'
import { ActionRow } from '../components/ActionRow.tsx'
import { useApp } from '../context/AppContext.tsx'
import { errorMessage, fmtFirmware } from '../lib/format.ts'
import { applyTheme, type ThemeChoice } from '../lib/theme.ts'

const TRANSPORTS: Record<string, [string, string]> = {
  udp: [
    'Directo',
    'Habla con el termostato por tu red, sin contraseñas. Es el modo recomendado.',
  ],
  session: [
    'Con correo autorizado',
    'Igual que el directo, pero el termostato comprueba que tu correo esté autorizado. Útil si algún día el modo directo deja de funcionar.',
  ],
  sdk: [
    'Librería oficial de Nabto',
    'La misma que usa la app de Sorel. No está incluida en esta instalación.',
  ],
}

type SettingsConfig = {
  transport: string
  host: string
  email: string
  active: string
  available: string[]
  addr: string
}

export function SettingsView() {
  const { S, api, toast } = useApp()
  const [cfg, setCfg] = useState<SettingsConfig | null>(null)
  const [loadErr, setLoadErr] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [advOpen, setAdvOpen] = useState(false)
  const [theme, setTheme] = useState<ThemeChoice>(() => applyTheme())
  const [draft, setDraft] = useState({ transport: 'udp', host: '', email: '' })

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const c = await api<SettingsConfig>('/api/settings')
        if (cancelled) return
        setCfg(c)
        setDraft({ transport: c.transport, host: c.host || '', email: c.email || '' })
      } catch (e) {
        if (!cancelled) setLoadErr(errorMessage(e))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [api])

  const onTheme = useCallback((k: ThemeChoice) => {
    setTheme(applyTheme(k))
  }, [])

  if (loadErr) {
    return <div className="banner">No pude leer los ajustes: {loadErr}</div>
  }

  if (!cfg || !S) {
    return (
      <div className="empty">
        <span className="spin" /> Leyendo ajustes…
      </div>
    )
  }

  const ip = cfg.addr.split(':')[0]

  return (
    <>
      <div className="page-head">
        <h3>Ajustes</h3>
      </div>
      <div className="card conn">
        <span className={`live ${S.error ? 'err' : ''}`} />
        <div>
          <b>{S.error ? 'El termostato no responde' : 'Conectado al termostato'}</b>
          <div className="muted small">
            {ip} · {TRANSPORTS[cfg.active]?.[0] || cfg.active} · firmware del{' '}
            {fmtFirmware(S.firmware)}
          </div>
          {S.device_calls ? (
            <div className="muted small">
              {S.device_calls.last_minute} consultas al termostato en el último minuto ·{' '}
              {S.device_calls.total} desde que arrancó el servidor
            </div>
          ) : null}
        </div>
      </div>
      <ActionRow
        items={[
          {
            tag: 'a',
            href: '/backups',
            icon: 'archive',
            label: 'Copias de seguridad',
            sub: 'Guardar y restaurar la configuración',
          },
          {
            tag: 'a',
            href: '/info',
            icon: 'bell',
            label: 'Avisos',
            sub: 'Lo que ha pasado en el equipo',
          },
        ]}
      />
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
        {Object.entries(zoneRooms(S)).map(([z, r]) => (
          <div className="kv" key={z}>
            <span>Zona {z}</span>
            <b>{roomTitle(r)}</b>
          </div>
        ))}
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
      <h2 className="group-title">Apariencia</h2>
      <div className="card">
        <div className="spread">
          <div>
            <b>Tema</b>
            <div className="muted small">«Sistema» sigue el modo claro u oscuro del móvil</div>
          </div>
          <div className="seg" id="themeSeg">
            {(
              [
                ['system', 'Sistema'],
                ['light', 'Claro'],
                ['dark', 'Oscuro'],
              ] as const
            ).map(([k, l]) => (
              <button
                key={k}
                type="button"
                data-theme-set={k}
                className={k === theme ? 'active' : ''}
                onClick={() => onTheme(k)}
              >
                {l}
              </button>
            ))}
          </div>
        </div>
      </div>
      <details
        className="card section adv"
        id="adv"
        open={advOpen}
        onToggle={(e) => setAdvOpen(e.currentTarget.open)}
      >
        <summary>Avanzado: cómo conectar con el termostato</summary>
        <div className="radio-list">
          {Object.entries(TRANSPORTS).map(([k, [title, text]]) => {
            const ok = cfg.available.includes(k)
            return (
              <label key={k} className={`radio ${ok ? '' : 'disabled'}`}>
                <input
                  type="radio"
                  name="transport"
                  value={k}
                  checked={draft.transport === k}
                  disabled={!ok}
                  onChange={() => setDraft((d) => ({ ...d, transport: k }))}
                />
                <span>
                  <b>{title}</b>
                  {ok ? '' : ' '}
                  {!ok ? <span className="muted small">no disponible aquí</span> : null}
                  <br />
                  <span className="muted small">{text}</span>
                </span>
              </label>
            )
          })}
        </div>
        <label className="field">
          <span>IP del termostato</span>
          <input
            id="sHost"
            inputMode="decimal"
            value={draft.host}
            placeholder="Buscar automáticamente"
            onChange={(e) => setDraft((d) => ({ ...d, host: e.target.value }))}
          />
        </label>
        <label
          className="field"
          id="emailRow"
          style={{ display: draft.transport === 'session' ? '' : 'none' }}
        >
          <span>Correo autorizado</span>
          <input
            id="sEmail"
            type="email"
            value={draft.email}
            placeholder="tu-correo@ejemplo.com"
            onChange={(e) => setDraft((d) => ({ ...d, email: e.target.value }))}
          />
          <small className="muted">
            Tiene que estar en Experto › Ajustes › WiFi › Control de acceso.
          </small>
        </label>
        <div className="actions" style={{ marginTop: 14 }}>
          <button
            type="button"
            className="btn primary"
            id="sSave"
            disabled={busy}
            onClick={async () => {
              if (busy) return
              setBusy(true)
              try {
                const next = await api<SettingsConfig>('/api/settings', {
                  body: {
                    transport: draft.transport,
                    host: draft.host.trim(),
                    email: draft.email.trim(),
                  },
                })
                setCfg(next)
                setDraft({
                  transport: next.transport,
                  host: next.host || '',
                  email: next.email || '',
                })
                toast('Conectado en modo ' + TRANSPORTS[next.active]![0]!.toLowerCase())
              } catch (e) {
                toast('No se aplicó: ' + errorMessage(e), 7000)
              }
              setBusy(false)
            }}
          >
            {busy ? (
              <>
                <span className="spin" /> Probando…
              </>
            ) : (
              'Probar y guardar'
            )}
          </button>
        </div>
        <p className="muted small">
          Si el termostato no responde con el modo nuevo, se queda el que tenías.
        </p>
      </details>
    </>
  )
}
