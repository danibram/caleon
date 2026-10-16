import { useQuery } from '@tanstack/react-query'
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Link } from 'react-router'
import { Banner } from '../components/Banner.tsx'
import { Icon } from '../components/Icon.tsx'
import { useApp, type Snapshot } from '../context/AppContext.tsx'
import { errorMessage, es, shortErr } from '../lib/format.ts'
import { roomTitle } from '../lib/rooms.ts'
import { segments } from '../lib/schedule.ts'

type BackupEntry = {
  name: string
  created: number
  reason: string
  size: number
}

type RestoreChange = {
  what: string
  note?: string
  from?: string | string[]
  to?: string | string[]
}

type MenuDiff = { path: string; current: string; backup: string }

type RestoreResult = {
  changes: RestoreChange[]
  menu_differences: MenuDiff[]
  applied?: boolean
}

const dayLabel = (ts: number) => {
  const d = new Date(ts * 1000)
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const diff = Math.round((new Date(d).setHours(0, 0, 0, 0) - today.getTime()) / 864e5)
  return diff === 0
    ? 'Hoy'
    : diff === -1
      ? 'Ayer'
      : d.toLocaleDateString('es-ES', { weekday: 'long', day: 'numeric', month: 'long' })
}

const clock = (ts: number) =>
  new Date(ts * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })

function miniTimer(slots: string[] | undefined) {
  if (!slots) return null
  return (
    <span className="mt">
      {segments(slots).map((g) => (
        <i key={g.start} className={`slot-${g.mode}`} style={{ flex: g.end - g.start }} />
      ))}
    </span>
  )
}

export function BackupsView() {
  const { S, api, toast, ask, rereadSoon, reloadState } = useApp()
  const backups = useQuery({
    queryKey: ['backups'],
    queryFn: () => api<BackupEntry[]>('/api/backups'),
    retry: false,
  })
  const list = backups.isError ? [] : (backups.data ?? null)
  const [open, setOpen] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [result, setResult] = useState<Record<string, ReactNode>>({})
  const progressRef = useRef<{ step: HTMLSpanElement | null; bar: HTMLElement | null }>({
    step: null,
    bar: null,
  })
  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const progressTick = useCallback(() => {
    const tick = setInterval(async () => {
      try {
        const st = await api<Snapshot>('/api/state', { any: true })
        const done = st.crawl_done || 0
        const exp = st.crawl_expected || 0
        const step = progressRef.current.step
        const bar = progressRef.current.bar
        if (step)
          step.textContent = exp
            ? `Leyendo ajustes del equipo: ${done} de ${exp}`
            : `Leyendo ajustes del equipo: ${done}`
        if (bar && exp) bar.style.width = Math.min(98, (done / exp) * 100) + '%'
      } catch {
        /* ignore */
      }
    }, 1500)
    tickRef.current = tick
    return tick
  }, [api])

  const { refetch } = backups
  const load = useCallback(async () => {
    await reloadState()
    await refetch()
  }, [reloadState, refetch])

  // the list query starts on its own; the snapshot refresh runs alongside it instead of before it
  useEffect(() => {
    void reloadState()
  }, [reloadState])

  const listError = backups.error
  useEffect(() => {
    if (listError) toast(listError.message)
  }, [listError, toast])

  const diffHtml = useCallback(
    (r: RestoreResult, title?: string) => {
      if (!S) return null
      const n = r.changes.length + r.menu_differences.length
      const named = (p: string) =>
        String(p).replace(/Habitación \d+/g, (name) => {
          const rr = S.rooms.find((x) => x.name === name)
          return rr ? roomTitle(rr) : name
        })
      return (
        <div className={`bk-result ${n ? '' : 'ok'}`}>
          <div className="bk-res-title">
            {n ? (
              `${n} diferencia${n > 1 ? 's' : ''} con el equipo`
            ) : (
              <>
                <Icon name="check" className="mini" /> Sin cambios de configuración: todo coincide
                con el equipo
              </>
            )}
          </div>
          {r.applied && title ? <div className="muted small">{title}</div> : null}
          {n ? (
            <ul className="diff">
              {r.changes.map((c, i) => (
                <li key={i}>
                  <b>{c.what}</b>
                  {c.note ? (
                    <span className="muted"> · {c.note}</span>
                  ) : Array.isArray(c.to) ? (
                    <div className="mini-tl">
                      {miniTimer(Array.isArray(c.from) ? c.from : undefined)}
                      {miniTimer(c.to)}
                    </div>
                  ) : (
                    <div className="muted">
                      {c.from} → <b>{c.to}</b>
                    </div>
                  )}
                </li>
              ))}
              {r.menu_differences.map((c, i) => (
                <li key={'m' + i}>
                  <b>{named(c.path)}</b>
                  <div className="muted">
                    ahora {es(c.current)} · en la copia <b>{es(c.backup)}</b>
                  </div>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      )
    },
    [S],
  )

  const compare = async ({
    name,
    apply,
    backup,
  }: {
    name: string
    apply: boolean
    backup?: unknown
  }) => {
    if (busy) return
    setBusy(name)
    if (name !== 'upload.json') setOpen(name)
    const tick = progressTick()
    const key = name === 'upload.json' ? 'upload' : name
    try {
      const body: { apply: boolean; backup?: unknown } = { apply: false }
      if (backup) body.backup = backup
      const r = await api<RestoreResult>(`/api/backups/${encodeURIComponent(name)}/restore`, {
        body,
      })
      setResult((prev) => ({ ...prev, [key]: diffHtml(r) }))
      if (apply) {
        setBusy(null)
        if (!r.changes.length) {
          toast('No hay nombres ni horarios que restaurar')
          return
        }
        const ok = await ask({
          title: '¿Restaurar esta copia?',
          text: `Se escribirán ${r.changes.length} cambios de nombres y horarios en el termostato.`,
          input: false,
          ok: 'Restaurar',
        })
        if (!ok) return
        setBusy(name)
        const applied = await api<RestoreResult>(
          `/api/backups/${encodeURIComponent(name)}/restore`,
          { body: { ...body, apply: true } },
        )
        setResult((prev) => ({ ...prev, [key]: diffHtml(applied, 'Restaurado') }))
        rereadSoon()
        toast('Copia restaurada')
      }
    } catch (e) {
      const msg = errorMessage(e)
      const lost = /sin respuesta|no respond/i.test(msg)
      setResult((prev) => ({
        ...prev,
        [key]: (
          <div className="bk-result err">
            <div className="bk-res-title">
              {lost
                ? 'El termostato dejó de responder mientras lo leíamos'
                : 'No se pudo comparar'}
            </div>
            <div className="muted small">
              {lost
                ? 'Suele pasar si alguien usa el termostato o la app a la vez. Prueba de nuevo en un momento.'
                : shortErr(msg)}
            </div>
            {name !== 'upload.json' ? (
              <button
                type="button"
                className="btn"
                style={{ marginTop: 10 }}
                data-cmp={name}
                onClick={() => compare({ name, apply: false })}
              >
                Reintentar
              </button>
            ) : null}
          </div>
        ),
      }))
    } finally {
      clearInterval(tick)
      tickRef.current = null
      setBusy(null)
      await load()
    }
  }

  if (!S) return null

  const last = list?.[0]

  return (
    <>
      <Banner />
      <Link className="back" to="/settings">
        <Icon name="back" className="mini" /> Ajustes
      </Link>
      <div className="page-head">
        <h3>Copias de seguridad</h3>
        <button
          type="button"
          className="btn primary"
          id="bNew"
          disabled={!!busy || !!S.crawling}
          onClick={async () => {
            if (busy) return
            setBusy('new')
            const tick = progressTick()
            try {
              const r = await api<{ name: string }>('/api/backups', { body: {} })
              toast('Copia hecha')
              setOpen(r.name)
            } catch (e) {
              toast('No se pudo copiar: ' + errorMessage(e), 6000)
            } finally {
              clearInterval(tick)
            }
            setBusy(null)
            await load()
          }}
        >
          {busy === 'new' ? (
            <>
              <span className="spin" /> Copiando…
            </>
          ) : (
            'Hacer copia ahora'
          )}
        </button>
      </div>
      <p className="lead">
        {!list
          ? 'Cargando…'
          : (last
              ? `Última copia: ${dayLabel(last.created).toLowerCase()} a las ${clock(last.created)}. `
              : 'Todavía no hay copias. Hacer una tarda alrededor de un minuto. ') +
            (S.backup_hours
              ? `Cada ${S.backup_hours === 24 ? 'día' : S.backup_hours + ' horas'} el servidor revisa la configuración y guarda una copia solo si algo ha cambiado.`
              : 'Las copias automáticas están desactivadas en este servidor.')}
      </p>
      {busy === 'new' ? (
        <div className="bk-progress standalone">
          <span
            ref={(el) => {
              progressRef.current.step = el
            }}
          >
            Leyendo la configuración del equipo…
          </span>
          <div className="bar determinate">
            <i
              ref={(el) => {
                progressRef.current.bar = el
              }}
            />
          </div>
        </div>
      ) : null}
      <div className="bk-list stagger">
        {(list || []).map((b, k) => (
          <div
            key={b.name}
            className={`bk ${open === b.name ? 'open' : ''}`}
            style={{ '--k': k }}
          >
            <button
              type="button"
              className="bk-row"
              aria-expanded={open === b.name}
              onClick={() => setOpen(open === b.name ? null : b.name)}
            >
              <span className="bk-ic">
                <Icon name="archive" />
              </span>
              <span className="bk-main">
                <b>
                  {dayLabel(b.created)}, {clock(b.created)}
                </b>
                <span className="muted small">
                  {b.reason === 'manual' ? 'Manual' : 'Automática'} · {Math.round(b.size / 1024)}{' '}
                  KB
                </span>
              </span>
              <Icon name="chevron" className="chev" />
            </button>
            {open === b.name ? (
              <>
                <div className="bk-actions">
                  <a className="btn" href={`/api/backups/${encodeURIComponent(b.name)}`}>
                    Descargar
                  </a>
                  <button
                    type="button"
                    className="btn"
                    disabled={!!busy}
                    onClick={() => compare({ name: b.name, apply: false })}
                  >
                    Comparar
                  </button>
                  <button
                    type="button"
                    className="btn"
                    disabled={!!busy}
                    onClick={() => compare({ name: b.name, apply: true })}
                  >
                    Restaurar…
                  </button>
                </div>
                <div className="bk-danger">
                  <button
                    type="button"
                    className="btn ghost warn"
                    disabled={!!busy}
                    onClick={async () => {
                      const ok = await ask({
                        title: '¿Borrar esta copia?',
                        text: 'No se puede deshacer.',
                        input: false,
                        ok: 'Borrar',
                      })
                      if (!ok) return
                      try {
                        await api(`/api/backups/${encodeURIComponent(b.name)}`, {
                          method: 'DELETE',
                        })
                        setOpen(null)
                        await load()
                      } catch (e) {
                        toast(errorMessage(e))
                      }
                    }}
                  >
                    Borrar esta copia
                  </button>
                </div>
                {busy === b.name ? (
                  <div className="bk-progress">
                    <span
            ref={(el) => {
              progressRef.current.step = el
            }}
          >
                      Leyendo la configuración del equipo para compararla…
                    </span>
                    <div className="bar determinate">
                      <i
              ref={(el) => {
                progressRef.current.bar = el
              }}
            />
                    </div>
                  </div>
                ) : null}
                {result[b.name]}
              </>
            ) : null}
          </div>
        ))}
      </div>
      <div className="bk-upload">
        <label className="btn">
          Comparar un archivo de copia…
          <input
            type="file"
            id="bUpload"
            accept=".json,application/json"
            hidden
            onChange={async (ev) => {
              const f = ev.target.files?.[0]
              if (!f) return
              try {
                const backup: unknown = JSON.parse(await f.text())
                await compare({ name: 'upload.json', apply: false, backup })
              } catch {
                toast('Ese archivo no es una copia válida', 6000)
              }
            }}
          />
        </label>
        {result.upload}
      </div>
      <p className="muted small" style={{ marginTop: 18 }}>
        Restaurar devuelve los nombres de las habitaciones y sus horarios. Los ajustes del menú
        Experto solo se comparan, porque el equipo no deja escribirlos de golpe. Se cambian uno a uno
        desde Experto.
      </p>
    </>
  )
}
