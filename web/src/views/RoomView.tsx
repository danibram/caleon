import { useState } from 'react'
import { Link, useParams } from 'react-router'
import { RangeSeg } from '../charts/RangeSeg.tsx'
import { RoomChart } from '../charts/RoomChart.tsx'
import { bestRange } from '../charts/chartData.ts'
import { ActionRow, type ActionItem } from '../components/ActionRow.tsx'
import { Banner } from '../components/Banner.tsx'
import { Band } from '../components/Band.tsx'
import { Gauge } from '../components/Gauge.tsx'
import { Icon } from '../components/Icon.tsx'
import { RoomControls } from '../components/RoomControls.tsx'
import { RoomIcon } from '../components/RoomIcon.tsx'
import { useApp } from '../context/AppContext.tsx'
import { useRoomChange, useRoomControlGuard } from '../hooks/useRoomChange.ts'
import { SYSTEM } from '../lib/constants.ts'
import { es, fmtC, num } from '../lib/format.ts'
import { isPresent } from '../lib/guards.ts'
import { Forecast, UnknownModesNote } from '../lib/forecast.tsx'
import { modeOf, roomById, roomTitle, stateLine } from '../lib/rooms.ts'
import { vacationPresets } from '../lib/schedule.ts'

export function RoomView() {
  const { id: idParam } = useParams()
  const { S, ask } = useApp()
  const roomChange = useRoomChange()
  const { staleData, roomBusy: busySet, blockedToast } = useRoomControlGuard()

  const id = Number(idParam)
  const r = roomById(S, id)

  const [roomHours, setRoomHours] = useState(() =>
    S ? bestRange({ ranges: [6, 24, 168, 720], want: 24, state: S }) : 24,
  )

  if (!S) return null

  if (!r) {
    return (
      <div className="empty">
        No encuentro esa habitación. <Link to="/">Volver al inicio</Link>
      </div>
    )
  }

  const isSys = r.id === SYSTEM
  const holiday = !!r.vacation
  const mode = modeOf(r)
  const canAdjust = !isSys && mode !== 'off' && !holiday
  const seasonFeature = S.features?.season_menu
  const off = mode === 'off'
  const t = num(r.temperature)
  const st = stateLine(r)

  const candidates: (ActionItem | null)[] = [
    !isSys && !holiday
      ? {
          tag: 'a',
          href: `#/timer/${r.id}`,
          icon: 'clock',
          label: 'Horarios',
          sub: 'Programa de cada día',
        }
      : null,
    isSys && S.features?.dhw_timer && !holiday
      ? {
          tag: 'a',
          href: `#/timer/${r.id}`,
          icon: 'clock',
          label: 'Horario de agua caliente',
          sub: '',
        }
      : null,
    !holiday
      ? {
          tag: 'button',
          id: 'holidayOn',
          icon: 'plane',
          label: 'Vacaciones',
          sub: 'Pausar hasta una fecha',
          onClick: async () => {
            const presets = vacationPresets()
            const v = await ask({
              title: 'Vacaciones hasta…',
              text:
                modeOf(r) === 'off'
                  ? 'Mientras dure, la habitación queda en modo vacaciones. Al terminar vuelve sola a su programa.'
                  : 'Deja de seguir su programa hasta esa fecha y después vuelve sola a él.',
              type: 'datetime-local',
              value: presets[2]![1],
              presets,
              ok: 'Activar',
              label: 'Fecha y hora de vuelta',
            })
            if (typeof v === 'string' && v) {
              void roomChange(r.id, { vacation: v.replace('T', ' ').slice(0, 16) })
            }
          },
        }
      : {
          tag: 'button',
          id: 'holidayOff',
          icon: 'plane',
          label: 'Terminar vacaciones',
          sub: 'Hasta ' + es(r.vacation),
          onClick: async () => {
            if (await ask({ title: '¿Terminar las vacaciones?', input: false, ok: 'Sí' })) {
              void roomChange(r.id, { vacation: null })
            }
          },
        },
    seasonFeature && !holiday
      ? {
          tag: 'button',
          id: 'season',
          icon: 'swap',
          label: r.season === 'cooling' ? 'Pasar a calefacción' : 'Pasar a refrigeración',
          sub: '',
          onClick: async () => {
            const to = r.season === 'cooling' ? 'heating' : 'cooling'
            if (
              await ask({
                title: to === 'cooling' ? '¿Cambiar a refrigeración?' : '¿Cambiar a calefacción?',
                text: 'El cambio de estación tarda un rato en aplicarse.',
                input: false,
                ok: 'Cambiar',
              })
            ) {
              void roomChange(r.id, { season: to })
            }
          },
        }
      : null,
  ]
  const actions = candidates.filter(isPresent)

  const houseRows = isSys
    ? S.rooms
        .filter((x) => x.type !== 'outdoor')
        .map((x) => {
          const xst = stateLine(x)
          return (
            <Link key={x.id} className="house-row" to={`/room/${x.id}`}>
              <RoomIcon room={x} className={xst.cls === 'heat' ? 'hot' : ''} />
              <span className="rc-name">
                <b>{roomTitle(x)}</b>
                <span className={`rc-sub st-${xst.cls}`}>
                  {xst.cls === 'heat' ? <Icon name="flame" className="mini" /> : <span className="sdot" />}
                  {xst.text}
                </span>
              </span>
              <span className="hr-temp">
                {fmtC(num(x.temperature))}
                <small style={{ fontSize: 12, color: 'var(--muted)' }}>°</small>
              </span>
              <Icon name="chevron" className="chev" />
            </Link>
          )
        })
    : null

  return (
    <>
      <Banner />
      <Link className="back" to="/">
        <Icon name="back" className="mini" /> Inicio
      </Link>
      <div className="room-layout">
        <div className="room-hero" data-room={r.id}>
          <div className="row">
            <RoomIcon room={r} size="lg" className={st.cls === 'heat' ? 'hot' : ''} />
            <div>
              <h3>{roomTitle(r)}</h3>
              <div className={`rc-sub st-${st.cls}`}>
                {isSys ? 'Un modo para todas las habitaciones' : st.text}
              </div>
            </div>
          </div>
          {isSys ? (
            <div className="house-list">{houseRows}</div>
          ) : off ? (
            <>
              <div className="offpanel">
                <div className="op-temp" data-vt-hero style={{ viewTransitionName: 'room-temp' }}>
                  {fmtC(t)}
                  <small>°</small>
                </div>
                <div className="op-label">temperatura actual</div>
                {r.humidity && r.humidity !== '--' ? (
                  <div className="hum-line">
                    <Icon name="drop" className="mini" /> Humedad {es(r.humidity)}
                  </div>
                ) : null}
              </div>
              <div className="room-modes off-modes">
                <div className="rm-label">Elige un modo para encenderla</div>
                <RoomControls room={r} S={S} compact="modes" />
                <Forecast room={r} S={S} />
                <UnknownModesNote room={r} S={S} />
              </div>
              <Band room={r} S={S} big />
            </>
          ) : (
            <>
              <Gauge room={r} />
              <div className="dial-controls">
                <button
                  type="button"
                  className="round"
                  disabled={!canAdjust}
                  aria-label="Bajar el objetivo medio grado"
                  onClick={() => {
                    if (staleData) {
                      blockedToast()
                      return
                    }
                    if (busySet.has(r.id)) return
                    void roomChange(r.id, { tdelta: -1 })
                  }}
                >
                  −
                </button>
                <div className="dial-hint">{holiday ? 'En vacaciones' : 'Medio grado por toque'}</div>
                <button
                  type="button"
                  className="round"
                  disabled={!canAdjust}
                  aria-label="Subir el objetivo medio grado"
                  onClick={() => {
                    if (staleData) {
                      blockedToast()
                      return
                    }
                    if (busySet.has(r.id)) return
                    void roomChange(r.id, { tdelta: 1 })
                  }}
                >
                  +
                </button>
              </div>
              {r.humidity && r.humidity !== '--' ? (
                <div className="hum-line">
                  <Icon name="drop" className="mini" /> Humedad {es(r.humidity)}
                </div>
              ) : null}
              <Forecast room={r} S={S} />
              <Band room={r} S={S} big />
            </>
          )}
          {off && !isSys ? null : (
            <div className="room-modes">
              <div className="rm-label">{isSys ? 'Elige un modo para toda la casa' : 'Modo'}</div>
              <RoomControls room={r} S={S} compact="modes" />
            </div>
          )}
        </div>
        <div className="room-side">
          <h2 className="group-title">Programación</h2>
          <ActionRow items={actions} />
          {!isSys && S.features?.room_renaming ? (
            <>
              <h2 className="group-title">Habitación</h2>
              <ActionRow
                items={[
                  {
                    tag: 'button',
                    id: 'rename',
                    icon: 'pencil',
                    label: 'Cambiar nombre',
                    sub: 'Se guarda también en el termostato',
                    onClick: async () => {
                      const name = await ask({
                        title: 'Cambiar nombre',
                        text: 'Se guarda en el termostato, así que también lo verás en su pantalla y en la app oficial.',
                        value: roomTitle(r),
                        ok: 'Guardar',
                        label: 'Nombre',
                        max: 20,
                      })
                      if (typeof name === 'string' && name.trim() && name.trim() !== r.name) {
                        void roomChange(r.id, { name: name.trim() })
                      }
                    },
                  },
                ]}
              />
            </>
          ) : null}
          <div className="card section chart-card">
            <div className="chart-head">
              <b>{isSys ? 'Temperatura de la casa' : 'Histórico'}</b>
              <RangeSeg
                id="rRange"
                ranges={[6, 24, 168, 720]}
                current={roomHours}
                state={S}
                onChange={setRoomHours}
              />
            </div>
            <RoomChart
              hours={roomHours}
              state={S}
              onlyRoom={isSys ? null : String(r.id)}
              svgId="rChart"
              style={{ height: 220 }}
            />
          </div>
        </div>
      </div>
    </>
  )
}
