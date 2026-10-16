import {
  Fragment,
  useCallback,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useRef,
  useState,
  type MutableRefObject,
  type ReactNode,
} from 'react'
import { useLocation, useNavigate, useNavigationType } from 'react-router'
import { Banner } from '../components/Banner'
import { Icon } from '../components/Icon'
import { RoomIcon } from '../components/RoomIcon'
import { useAppState } from '../context/AppContext'
import { useNow } from '../hooks/useNow'
import { DAYS } from '../lib/constants'
import { errorMessage, shortErr } from '../lib/format'
import { hasMenuSync, MENU_SYNC_STATE } from '../lib/menuSync'
import { roomTitle, zoneRooms } from '../lib/rooms'
import { ServiceTimer } from './menu/ServiceTimer'
import { parseMenuBody, type MenuBody, type MenuResponse, type MenuScreen } from './menu/screens'
import {
  MENU_DANGER,
  MENU_HELP,
  MENU_ICON,
  INFO_HEAD,
  entryPath,
  es,
  fmtClockSec,
  fmtIsoDate,
  infoLabel,
  onOff,
  prettyTitle,
  tileRoomName,
  unent,
  type MenuTrailItem,
} from './menu/helpers'

type MenuState = { menu: MenuScreen; body: MenuBody }

const toState = (r: MenuResponse): MenuState => ({
  menu: r.menu,
  body: parseMenuBody(r.menu.type, r.detail),
})

type MenuAction = { menuId: string; action: string; entry?: string }

function menuRootFromPath(pathname: string): string {
  if (pathname === '/menu' || pathname === '/menu/') return ''
  return pathname.replace(/^\/menu\/?/, '')
}

export default function MenuView() {
  const location = useLocation()
  const navType = useNavigationType()
  const [boot, setBoot] = useState(location.key)
  // URL updates that only mirror the screen already open keep it; any other navigation into the menu starts over
  const selfSync = navType === 'REPLACE' && hasMenuSync(location.state)
  if (!selfSync && boot !== location.key) setBoot(location.key)
  return <MenuScreen key={boot} root={menuRootFromPath(location.pathname)} />
}

function MenuScreen({ root }: { root: string }) {
  const { S, api, toast, rereadSoon, navGen } = useAppState()
  const location = useLocation()
  const navigate = useNavigate()
  // the device stores no year for a month/day entry; the picker needs one for its value
  const now = useNow()

  const [menuCur, setMenuCur] = useState<MenuState | null>(null)
  const [menuTrail, setMenuTrail] = useState<MenuTrailItem[]>(() =>
    root === 'status'
      ? [{ id: 'cal:1', title: 'Experto', entry: 'Experto' }]
      : [],
  )
  const [menuValues, setMenuValues] = useState<Record<string, string> | null>(
    null,
  )
  const [waitingBackup, setWaitingBackup] = useState(false)
  const [openError, setOpenError] = useState<string | null>(null)

  const menuBusyRef = useRef(false)
  const [menuBusy, setMenuBusy] = useState(false)
  const setMenuBusyFlag = (v: boolean) => {
    menuBusyRef.current = v
    setMenuBusy(v)
  }
  const pendingEntry = useRef<string | null>(null)
  // handlers read the newest screen, trail and URL through these without being re-created
  const menuCurRef = useRef(menuCur)
  const menuTrailRef = useRef(menuTrail)
  const pathnameRef = useRef(location.pathname)
  useLayoutEffect(() => {
    menuCurRef.current = menuCur
    menuTrailRef.current = menuTrail
    pathnameRef.current = location.pathname
  })
  // the root the menu was opened with; it only changes when the menu is opened again
  const rootRef = useRef(root)
  const booted = useRef(false)
  const dirtyRef = useRef(false)

  const syncUrl = useCallback(
    (trail: MenuTrailItem[]) => {
      const path = entryPath(trail)
      const h =
        rootRef.current === 'status' && trail.length <= 1
          ? '/menu/status'
          : '/menu' +
            (path.length
              ? '/p/' + path.map(encodeURIComponent).join('/')
              : '')
      if (pathnameRef.current === h) return
      navigate(h, { replace: true, state: MENU_SYNC_STATE })
    },
    [navigate],
  )

  const renderAfterNav = useCallback(
    (r: MenuState, trail: MenuTrailItem[]) => {
      setMenuCur(r)
      setMenuTrail(trail)
      syncUrl(trail)
    },
    [syncUrl],
  )

  const go = useCallback(
    async function go({ id, action }: { id: string; action: string }): Promise<void> {
      const seq0 = navGen()
      try {
        const r = await api<MenuResponse>('/api/menu/action', {
          body: { id, action },
        })
        if (seq0 !== navGen()) return
        if (
          !(action === '' ||
            /^goto:/.test(action) ||
            ['back', 'forward', 'exit', 'status'].includes(action))
        )
          rereadSoon()
        if (!r.menu.id || action === 'exit') {
          setMenuCur(null)
          setMenuTrail([])
          rootRef.current = ''
          return go({ id: '', action: '' })
        }
        const prev = menuCurRef.current
        const trail = [...menuTrailRef.current]
        const isBack =
          prev && action === prev.menu.back && action !== 'exit'
        const at = trail.findIndex((x) => x.id === r.menu.id)
        let nextTrail: MenuTrailItem[]
        if (isBack && at < 0 && trail.length > 1) {
          trail.pop()
          trail[trail.length - 1].id = r.menu.id
          nextTrail = trail
        } else if (at >= 0) nextTrail = trail.slice(0, at + 1)
        else if (r.menu.id === 'cal:1' || (!prev && !trail.length))
          nextTrail = [
            { id: r.menu.id, title: r.menu.title, entry: r.menu.title },
          ]
        else if (!prev)
          nextTrail = [
            ...trail,
            { id: r.menu.id, title: r.menu.title, entry: r.menu.title },
          ]
        else if (r.menu.id !== prev.menu.id)
          nextTrail = [
            ...trail,
            {
              id: r.menu.id,
              title: r.menu.title,
              entry: pendingEntry.current || r.menu.title,
            },
          ]
        else nextTrail = trail
        pendingEntry.current = null
        renderAfterNav(toState(r), nextTrail)
        setWaitingBackup(false)
        setOpenError(null)
      } catch (e) {
        if (seq0 !== navGen()) return
        const msg = errorMessage(e)
        if (/leyendo la configuración/i.test(msg)) {
          setWaitingBackup(true)
          window.setTimeout(() => {
            if (seq0 === navGen()) void go({ id, action })
          }, 4000)
          return
        }
        toast('No se aplicó · ' + shortErr(msg), 6000)
        if (!menuCurRef.current) setOpenError(shortErr(msg))
      }
    },
    [api, navGen, rereadSoon, renderAfterNav, toast],
  )

  const runAction = useCallback(
    ({ menuId, action, entry }: MenuAction) => {
      if (menuBusyRef.current || !action) return
      pendingEntry.current = entry ?? null
      setMenuBusyFlag(true)
      void go({ id: menuId, action }).finally(() => {
        setMenuBusyFlag(false)
      })
    },
    [go],
  )

  const openPath = useCallback(
    async (path: string[]) => {
      const seq0 = navGen()
      const tryOpen = async () => {
        const r = await api<MenuResponse>('/api/menu/open', { body: { path } })
        if (seq0 !== navGen()) return
        const trail: MenuTrailItem[] = [
          { id: 'cal:1', title: 'Experto', entry: 'Experto' },
          ...path.map((e, k) => ({
            id: k === path.length - 1 ? r.menu.id : '?' + k,
            title: e,
            entry: e,
          })),
        ]
        renderAfterNav(toState(r), trail)
        setOpenError(null)
      }
      try {
        await tryOpen()
      } catch {
        await new Promise((res) => setTimeout(res, 900))
        if (seq0 !== navGen()) return
        try {
          await tryOpen()
        } catch {
          await go({ id: '', action: '' })
        }
      }
    },
    [api, go, navGen, renderAfterNav],
  )

  useEffect(() => {
    if (menuValues !== null) return
    void api<{ values?: Record<string, string> }>('/api/menu/values')
      .then((v) => {
        setMenuValues(v.values || {})
        if (menuCurRef.current) setMenuCur({ ...menuCurRef.current })
      })
      .catch(() => setMenuValues({}))
  }, [api, menuValues])

  const bootMenu = useEffectEvent(() => {
    if (root.startsWith('p/')) {
      const path = root
        .slice(2)
        .split('/')
        .map(decodeURIComponent)
      void openPath(path)
    } else {
      void go({ id: '', action: root === 'status' ? 'status' : '' })
    }
  })

  useEffect(() => {
    // StrictMode mounts twice; opening the menu twice moves the device cursor
    if (booted.current) return
    booted.current = true
    bootMenu()
  }, [])

  useEffect(() => {
    const onInput = (ev: Event) => {
      const t = ev.target
      if (t instanceof HTMLElement && t.matches('#view input, #view select, #view textarea'))
        t.dataset.dirty = '1'
      dirtyRef.current = true
    }
    document.addEventListener('input', onInput)
    return () => document.removeEventListener('input', onInput)
  }, [])

  useEffect(() => {
    const refreshDetail = async () => {
      const cur = menuCurRef.current
      if (!cur || cur.menu.type === 'timer') return
      if (document.querySelector('dialog[open]')) return
      if (
        document.querySelector(
          '#view input:focus, #view select:focus, #view textarea:focus, [data-dirty]',
        )
      )
        return
      try {
        const detail = await api(
          `/api/menu/detail?id=${encodeURIComponent(cur.menu.id)}&type=${encodeURIComponent(cur.menu.type)}`,
        )
        setMenuCur((prev) =>
          prev ? { ...prev, body: parseMenuBody(prev.menu.type, detail) } : prev,
        )
      } catch {
        /* ignore */
      }
    }
    const id = window.setInterval(refreshDetail, 10000)
    return () => clearInterval(id)
  }, [api])

  const setMenuField = useCallback(
    async (body: Record<string, unknown>) => {
      const cur = menuCurRef.current
      if (!cur || menuBusyRef.current) return
      setMenuBusyFlag(true)
      try {
        const detail = await api('/api/menu/set', {
          body: { id: cur.menu.id, type: cur.menu.type, ...body },
        })
        setMenuCur({ ...cur, body: parseMenuBody(cur.menu.type, detail) })
        toast('Guardado')
        rereadSoon()
      } catch (e) {
        toast('Error: ' + errorMessage(e), 6000)
      } finally {
        setMenuBusyFlag(false)
      }
    },
    [api, rereadSoon, toast],
  )

  const onCrumb = useCallback(
    async ({ index, menuId }: { index: number; menuId: string }) => {
      if (index === 0) {
        runAction({ menuId, action: 'exit' })
        return
      }
      try {
        const r = await api<MenuResponse>('/api/menu/open', {
          body: { path: entryPath(menuTrailRef.current).slice(0, index) },
        })
        const trail = menuTrailRef.current.slice(0, index + 1)
        trail[index].id = r.menu.id
        renderAfterNav(toState(r), trail)
      } catch (e) {
        toast(errorMessage(e), 5000)
      }
    },
    [api, renderAfterNav, runAction, toast],
  )

  const handleTimerCopy = useCallback(
    async ({ day, mask }: { day: number; mask: number }) => {
      const cur = menuCurRef.current
      if (!cur || menuBusyRef.current) return
      setMenuBusyFlag(true)
      try {
        await api('/api/menu/set', {
          body: { id: cur.menu.id, type: 'timer copy', day, mask },
        })
        toast('Copiado')
        rereadSoon()
      } catch (e) {
        toast(errorMessage(e), 6000)
      } finally {
        setMenuBusyFlag(false)
      }
    },
    [api, rereadSoon, toast],
  )

  if (waitingBackup) {
    return (
      <div className="empty waiting">
        <span className="spin" />
        <b>Un momento</b>
        <span className="muted">
          El servidor está haciendo la copia diaria y está usando el menú del equipo. Se abre
          solo en cuanto termine (menos de un minuto).
        </span>
      </div>
    )
  }

  if (openError && !menuCur) {
    return (
      <div className="empty">
        <b>No se pudo abrir el menú</b>
        <span className="muted">{openError}</span>
        <button
          type="button"
          className="btn"
          id="mRetry"
          onClick={() => {
            setOpenError(null)
            void go({ id: '', action: rootRef.current === 'status' ? 'status' : '' })
          }}
        >
          Reintentar
        </button>
      </div>
    )
  }

  if (!menuCur) return null

  const m = menuCur.menu
  const d = menuCur.body
  const zr = zoneRooms(S)

  let body: ReactNode

  switch (d.kind) {
    case 'string list': {
      const base = entryPath(menuTrail)
      body = (
        <div className="list menu-list">
          {d.items.map((i) => {
            const raw = unent(i.title)
            const action = i.action
            const holdAction = i.hold_action
            let val = menuValues
              ? menuValues[[...base, raw].join(' › ')]
              : undefined
            if (
              raw === 'Habitación' &&
              /^Zona [A-Z]$/.test(base[base.length - 1] || '')
            ) {
              const zz = zr[base[base.length - 1].slice(-1)]
              val = zz ? roomTitle(zz) : 'Ninguna'
            }
            const zone = /^Zona ([A-Z])$/.exec(raw)
            const zroom =
              zone && zr[zone[1]]
                ? roomTitle(zr[zone[1]])
                : zone
                  ? 'sin habitación'
                  : ''
            const help = MENU_HELP[raw]
            const zoneRoom = zone && zr[zone[1]]
            const mi =
              MENU_ICON[raw] || (/^Zona [A-Z]$/.test(raw) ? 'zone' : 'sliders')
            return (
              <button
                type="button"
                key={raw + (action || '')}
                className={MENU_DANGER.test(raw) ? 'danger-row' : ''}
                disabled={!action}
                onClick={action ? () => runAction({ menuId: m.id, action, entry: raw }) : undefined}
              >
                {zoneRoom ? (
                  <RoomIcon room={zoneRoom} size="sm" />
                ) : mi ? (
                  <span className="mic">
                    <Icon name={mi} />
                  </span>
                ) : null}
                <span className="ml">
                  <span>
                    {prettyTitle(raw)}
                    {zroom ? (
                      <span className="muted"> · {zroom}</span>
                    ) : null}
                  </span>
                  {help ? <small>{help}</small> : null}
                </span>
                <span className="row">
                  {val !== undefined && val !== '' ? (
                    <span className="mval">
                      {/^[01]$/.test(val) ? (val === '1' ? 'Sí' : 'No') : onOff(es(val))}
                    </span>
                  ) : null}
                  {holdAction ? (
                    <span
                      className="btn small"
                      role="button"
                      tabIndex={0}
                      onClick={(ev) => {
                        ev.stopPropagation()
                        runAction({ menuId: m.id, action: holdAction })
                      }}
                      onKeyDown={(ev) => {
                        if (ev.key === 'Enter') {
                          ev.stopPropagation()
                          runAction({ menuId: m.id, action: holdAction })
                        }
                      }}
                    >
                      ⋯
                    </span>
                  ) : null}
                  <Icon name="chevron" className="chev" />
                </span>
              </button>
            )
          })}
        </div>
      )
      break
    }
    case 'tiles':
      body = (
        <>
          {d.hint ? <p className="muted">{d.hint}</p> : null}
          <div className="tiles">
            {d.items.map((i, idx) => {
              const action = i.action
              const holdAction = i.hold_action
              return (
                <div
                  key={idx}
                  className={`tile ${i.highlighted ? 'hl' : ''} ${action ? '' : 'disabled'}`}
                  role={action ? 'button' : undefined}
                  tabIndex={action ? 0 : undefined}
                  onClick={action ? () => runAction({ menuId: m.id, action }) : undefined}
                  onKeyDown={(ev) => {
                    if (action && (ev.key === 'Enter' || ev.key === ' '))
                      runAction({ menuId: m.id, action })
                  }}
                >
                  <img
                    src={`/icons/${i.icon || 'option_empty'}.svg`}
                    alt=""
                    onError={(ev) => {
                      ev.currentTarget.style.visibility = 'hidden'
                    }}
                  />
                  <b>{i.icon_text}</b>
                  <span className="small">{i.title || tileRoomName(i.icon, S)}</span>
                  {holdAction ? (
                    <button
                      type="button"
                      className="btn small"
                      onClick={(ev) => {
                        ev.stopPropagation()
                        runAction({ menuId: m.id, action: holdAction })
                      }}
                    >
                      ⋯
                    </button>
                  ) : null}
                </div>
              )
            })}
          </div>
        </>
      )
      break
    case 'options': {
      const { decrease, increase } = d
      body = (
        <>
          <p className="muted" style={{ textAlign: 'center' }}>
            {MENU_HELP[unent(m.title)] || es(d.hint)}
          </p>
          <div className="row" style={{ justifyContent: 'center', gap: 24 }}>
            <button
              type="button"
              className="round"
              disabled={!decrease}
              onClick={decrease ? () => runAction({ menuId: m.id, action: decrease }) : undefined}
            >
              −
            </button>
            <div className="big-value">{es(d.value)}</div>
            <button
              type="button"
              className="round"
              disabled={!increase}
              onClick={increase ? () => runAction({ menuId: m.id, action: increase }) : undefined}
            >
              +
            </button>
          </div>
        </>
      )
      break
    }
    case 'offset':
      body = (
        <>
          {d.hint ? <p className="muted">{d.hint}</p> : null}
          <table>
            <tbody>
              {d.rows.map((i, idx) => {
                const { decrease, increase } = i
                return (
                  <tr key={idx}>
                    <td>
                      <img
                        src={`/icons/${i.icon || 'option_empty'}.svg`}
                        alt=""
                        style={{ width: 28, height: 28 }}
                        onError={(ev) => {
                          ev.currentTarget.style.visibility = 'hidden'
                        }}
                      />
                    </td>
                    <td>{i.icon_text}</td>
                    <td className="num">{i.value}</td>
                    <td className="num">
                      <button
                        type="button"
                        className="round"
                        style={{ width: 32, height: 32, fontSize: 18 }}
                        disabled={!decrease}
                        onClick={
                          decrease ? () => runAction({ menuId: m.id, action: decrease }) : undefined
                        }
                      >
                        −
                      </button>
                      <b style={{ display: 'inline-block', minWidth: 60, textAlign: 'center' }}>
                        {i.offset}
                      </b>
                      <button
                        type="button"
                        className="round"
                        style={{ width: 32, height: 32, fontSize: 18 }}
                        disabled={!increase}
                        onClick={
                          increase ? () => runAction({ menuId: m.id, action: increase }) : undefined
                        }
                      >
                        +
                      </button>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </>
      )
      break
    case 'information':
      body = (
        <div className="info-rows">
          {d.lines.map((l, idx) => {
            const raw = unent(l.text).trim()
            const action = l.action
            const kv0 = /^(.+?): (.+)$/.exec(raw)
            let kv: [string, string, string] | null = null
            if (kv0) {
              const [lab, val] = infoLabel({ label: kv0[1], value: kv0[2], S })
              kv = [raw, lab, onOff(es(val))]
            }
            const headKey = raw.replace(/:$/, '')
            const head = INFO_HEAD[headKey]
            const text = fmtClockSec(
              fmtIsoDate(
                head
                  ? head + (raw.endsWith(':') ? ':' : '')
                  : onOff(es(raw)),
              ),
            )
            const rowProps = {
              className: action ? 'ir link' : 'ir',
              role: action ? 'button' : undefined,
              tabIndex: action ? 0 : undefined,
              onClick: action ? () => runAction({ menuId: m.id, action }) : undefined,
            }
            if (!text.trim()) return <div key={idx} className="ir-gap" />
            if (kv && /^(Exterior|Bomba)/.test(kv[1]))
              return (
                <Fragment key={idx}>
                  <div className="ir-head">General</div>
                  <div {...rowProps}>
                    <span>{kv[1]}</span>
                    <b>{kv[2]}</b>
                  </div>
                </Fragment>
              )
            if (kv)
              return (
                <div key={idx} {...rowProps}>
                  <span>{kv[1]}</span>
                  <b>{kv[2]}</b>
                </div>
              )
            if (
              /[a-záéíóúñ]/i.test(text) &&
              (text.trim().endsWith(':') || !/\d/.test(text) || INFO_HEAD[raw])
            )
              return (
                <div key={idx} className="ir-head">
                  {text.replace(/:$/, '')}
                </div>
              )
            return (
              <div key={idx} {...rowProps}>
                <span>{text}</span>
              </div>
            )
          })}
        </div>
      )
      break
    case 'binary query':
      body = (
        <>
          <div className="lines">
            {d.lines.map((line, i) => (
              <p key={i} className="center">
                {line}
              </p>
            ))}
          </div>
          <div
            className="row"
            style={{ justifyContent: 'center', gap: 12, marginTop: 16 }}
          >
            <button
              type="button"
              className={`btn ${d.value === d.yes ? 'primary' : ''}`}
              style={{ minWidth: 90 }}
              onClick={() => runAction({ menuId: m.id, action: d.yes })}
            >
              Sí
            </button>
            <button
              type="button"
              className={`btn ${d.value === d.no ? 'primary' : ''}`}
              style={{ minWidth: 90 }}
              onClick={() => runAction({ menuId: m.id, action: d.no })}
            >
              No
            </button>
          </div>
        </>
      )
      break
    case 'date': {
      const p = (n: number) => String(n).padStart(2, '0')
      const { clock } = d
      const year = d.yearless ? new Date(now).getFullYear() : clock.year
      const has = clock.day > 0 && clock.month > 0
      body = (
        <DateTimeForm
          hint={d.hint}
          showClear={d.yearless}
          onSubmit={(fields) => void setMenuField(fields)}
          menuBusy={menuBusyRef}
          initial={{ kind: 'date', value: has ? `${year}-${p(clock.month)}-${p(clock.day)}` : '' }}
        />
      )
      break
    }
    case 'time': {
      const p = (n: number) => String(n).padStart(2, '0')
      body = (
        <DateTimeForm
          onSubmit={(fields) => void setMenuField(fields)}
          menuBusy={menuBusyRef}
          showNow
          initial={{ kind: 'time', value: `${p(d.clock.hour)}:${p(d.clock.minute)}` }}
        />
      )
      break
    }
    case 'datetime': {
      const p = (n: number) => String(n).padStart(2, '0')
      const { clock } = d
      body = (
        <DateTimeForm
          onSubmit={(fields) => void setMenuField(fields)}
          menuBusy={menuBusyRef}
          showNow
          initial={{
            kind: 'datetime',
            value: `${clock.year}-${p(clock.month)}-${p(clock.day)}T${p(clock.hour)}:${p(clock.minute)}`,
          }}
        />
      )
      break
    }
    case 'text':
      body = (
        <TextForm
          initial={d.value}
          menuBusy={menuBusyRef}
          onSubmit={(value) => void setMenuField({ value })}
        />
      )
      break
    case 'timer':
      body = (
        <ServiceTimer
          words={d.words}
          menuBusy={menuBusy}
          onSetDay={({ day, slots }) => void setMenuField({ day, slots })}
          onCopy={(args) => void handleTimerCopy(args)}
        />
      )
      break
    case 'time ranges': {
      const hm = (v: number) =>
        `${String(Math.floor(v / 60)).padStart(2, '0')}:${String(v % 60).padStart(2, '0')}`
      body = (
        <>
          <p className="muted small">
            {DAYS[d.day] || ''} · solo lectura
          </p>
          <table>
            <thead>
              <tr>
                <th>Inicio</th>
                <th>{d.withStop ? 'Fin' : 'Modo'}</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {d.ranges.map((x, i) => (
                <tr key={i}>
                  <td className="num">{hm(x.start)}</td>
                  <td className="num">
                    {d.withStop ? hm(x.stop) : d.modes[x.mode] ?? x.mode}
                  </td>
                  <td>
                    {x.title} <span className="muted">{x.hint}</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )
      break
    }
    case 'raw':
      body = <pre className="small">{JSON.stringify(d.detail, null, 1)}</pre>
      break
    default: {
      const _exhaustive: never = d
      void _exhaustive
    }
  }

  const { back, back_hold, hold_action, forward_hold, forward } = m
  const nav = (
    <div className="menu-nav">
      {back &&
      !(back === 'exit' && menuTrail.length <= 1) &&
      menuTrail.length <= 1 ? (
        <button
          type="button"
          className="btn"
          onClick={() => runAction({ menuId: m.id, action: back })}
        >
          {back === 'exit' ? (
            'Cerrar menú'
          ) : (
            <>
              <Icon name="back" className="mini" /> Atrás
            </>
          )}
        </button>
      ) : null}
      {back_hold ? (
        <button
          type="button"
          className="btn"
          title="Pulsación larga en Atrás"
          onClick={() => runAction({ menuId: m.id, action: back_hold })}
        >
          Atrás largo
        </button>
      ) : null}
      {hold_action && d.kind !== 'information' ? (
        <button
          type="button"
          className="btn"
          title="Pulsación larga"
          onClick={() => runAction({ menuId: m.id, action: hold_action })}
        >
          Más opciones
        </button>
      ) : null}
      {forward_hold ? (
        <button
          type="button"
          className="btn"
          title="Pulsación larga en Siguiente"
          onClick={() => runAction({ menuId: m.id, action: forward_hold })}
        >
          Siguiente largo
        </button>
      ) : null}
      {forward ? (
        <button
          type="button"
          className="btn"
          onClick={() => runAction({ menuId: m.id, action: forward })}
        >
          Página siguiente <Icon name="chevron" className="mini" />
        </button>
      ) : null}
    </div>
  )

  const crumbs =
    menuTrail.length > 1 ? (
      <nav className="crumbs" aria-label="Ruta">
        <Icon name="back" className="mini crumb-back" />
        {menuTrail.slice(0, -1).map((c, j) => (
          <Fragment key={j}>
            <button
              type="button"
              className={`crumb ${j < menuTrail.length - 2 ? 'crumb-far' : ''}`}
              onClick={() => void onCrumb({ index: j, menuId: m.id })}
            >
              {prettyTitle(c.entry)}
            </button>
            <span className="crumb-sep">
              <Icon name="chevron" className="mini" />
            </span>
          </Fragment>
        ))}
      </nav>
    ) : null

  return (
    <>
      <Banner />
      {crumbs}
      <div className="page-head">
        <h3 title={m.id}>
          {prettyTitle(menuTrail[menuTrail.length - 1]?.entry || m.title)}
        </h3>
      </div>
      {MENU_HELP[unent(m.title)] && d.kind !== 'string list' && d.kind !== 'options' ? (
        <p className="muted" style={{ margin: '-8px 2px 14px' }}>
          {MENU_HELP[unent(m.title)]}
        </p>
      ) : null}
      <div className="card menu-card">{body}</div>
      {nav}
      <p className="muted small menu-foot">
        El mismo menú que la pantalla del termostato. Lo que cambies aquí se aplica al momento.
      </p>
    </>
  )
}

function DateTimeForm({
  hint,
  initial,
  showClear,
  showNow,
  onSubmit,
  menuBusy,
}: {
  hint?: string
  initial:
    | { kind: 'date'; value: string }
    | { kind: 'time'; value: string }
    | { kind: 'datetime'; value: string }
  showClear?: boolean
  showNow?: boolean
  onSubmit: (body: Record<string, number>) => void
  menuBusy: MutableRefObject<boolean>
}) {
  const [value, setValue] = useState(initial.value)

  const onSave = () => {
    if (menuBusy.current) return
    switch (initial.kind) {
      case 'date': {
        const [y, mo, da] = value.split('-').map(Number)
        if (!da) return
        onSubmit({ day: da, month: mo, year: y })
        break
      }
      case 'time': {
        const [h, mi] = value.split(':').map(Number)
        onSubmit({ hour: h, minute: mi })
        break
      }
      case 'datetime': {
        const v = new Date(value)
        onSubmit({
          day: v.getDate(),
          month: v.getMonth() + 1,
          year: v.getFullYear(),
          hour: v.getHours(),
          minute: v.getMinutes(),
        })
        break
      }
      default: {
        const _exhaustive: never = initial
        void _exhaustive
      }
    }
  }

  const onNow = () => {
    const v = new Date()
    const p = (n: number) => String(n).padStart(2, '0')
    if (initial.kind === 'time') {
      setValue(`${p(v.getHours())}:${p(v.getMinutes())}`)
      onSubmit({ hour: v.getHours(), minute: v.getMinutes() })
      return
    }
    setValue(
      `${v.getFullYear()}-${p(v.getMonth() + 1)}-${p(v.getDate())}T${p(v.getHours())}:${p(v.getMinutes())}`,
    )
    onSubmit({
      day: v.getDate(),
      month: v.getMonth() + 1,
      year: v.getFullYear(),
      hour: v.getHours(),
      minute: v.getMinutes(),
    })
  }

  const onClear = () => {
    onSubmit({ day: 0, month: 0, year: new Date().getFullYear() })
  }

  return (
    <>
      {hint ? <p className="muted">{hint}</p> : null}
      <div className="row">
        {initial.kind === 'date' ? (
          <input
            type="date"
            id="mDate"
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        ) : initial.kind === 'time' ? (
          <input
            type="time"
            id="mTime"
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        ) : (
          <input
            type="datetime-local"
            id="mDT"
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        )}
        <button type="button" className="btn primary" id="mSet" onClick={onSave}>
          Guardar
        </button>
        {showClear ? (
          <button type="button" className="btn" id="mClear" onClick={onClear}>
            Desactivar
          </button>
        ) : null}
        {showNow ? (
          <button type="button" className="btn" id="mNow" onClick={onNow}>
            {initial.kind === 'time' ? 'Hora actual' : 'Ahora'}
          </button>
        ) : null}
      </div>
    </>
  )
}

function TextForm({
  initial,
  onSubmit,
  menuBusy,
}: {
  initial: string
  onSubmit: (value: string) => void
  menuBusy: MutableRefObject<boolean>
}) {
  const [value, setValue] = useState(initial)
  return (
    <div className="row">
      <input
        id="mText"
        value={value}
        style={{ flex: 1 }}
        onChange={(e) => setValue(e.target.value)}
      />
      <button
        type="button"
        className="btn primary"
        id="mSet"
        onClick={() => {
          if (!menuBusy.current) onSubmit(value)
        }}
      >
        Guardar
      </button>
    </div>
  )
}
