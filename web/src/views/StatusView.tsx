import { Link } from 'react-router'
import { Banner } from '../components/Banner.tsx'
import { Icon } from '../components/Icon.tsx'
import { RoomIcon } from '../components/RoomIcon.tsx'
import { useApp, type StatusMetric } from '../context/AppContext.tsx'
import { es, fmtC, num } from '../lib/format.ts'
import {
  isHeating,
  roomTitle,
  sensorRoom,
  stateLine,
  zoneRooms,
} from '../lib/rooms.ts'

export function StatusView() {
  const { S } = useApp()

  if (!S) return null

  const st = S.status || []
  const find = (group: string, label: string) =>
    st.find((m) => m.group.startsWith(group) && m.label === label)
  const onoff = (m: StatusMetric | undefined) => m && m.value === 1
  const zr = zoneRooms(S)

  const rooms = S.rooms
    .filter((r) => r.type !== 'outdoor')
    .map((r) => {
      const zones = Object.entries(zr)
        .filter(([, x]) => x.id === r.id)
        .map(([z]) => ({
          z,
          m: st.find((m) => m.group === 'Salidas' && m.label.endsWith(`Zona ${z}`)),
        }))
      return { r, zones }
    })

  const flowSet = find('Bomba circuito', 'Ida nominal')
  const flow = find('Bomba circuito', 'Ida')
  const demand = st.find((m) => m.group === 'Salidas' && /Demanda/.test(m.label))
  const pumpOut = st.find((m) => m.group === 'Salidas' && /Bomba circuito/.test(m.label))
  const probes = st.filter((m) => m.group === 'Entradas' && /^\d+\.\d+$/.test(m.label))
  const outdoor = st.find((m) => m.label === 'Exterior')
  const freeZones = st.filter(
    (m) => m.group === 'Salidas' && /Zona [A-Z]$/.test(m.label) && !zr[m.label.slice(-1)],
  )
  const heatingRooms = rooms.filter((x) => x.zones.some((z) => onoff(z.m)))
  const anyHeat = heatingRooms.length > 0

  return (
    <>
      <Banner />
      <div className="page-head">
        <h3>Instalación</h3>
      </div>
      <p className="lead">
        {anyHeat
          ? `Ahora mismo calienta${heatingRooms.length > 1 ? 'n' : ''} ${heatingRooms.map((x) => roomTitle(x.r)).join(' y ')}.`
          : 'Ninguna zona está calentando ahora mismo.'}{' '}
        {onoff(demand)
          ? ' La centralita está pidiendo calor a la caldera.'
          : ' No se está pidiendo calor a la caldera.'}
      </p>
      <div className="flow">
        <section className="node">
          <div className="node-h">Habitaciones</div>
          {rooms.map(({ r }) => (
            <Link key={r.id} className="node-row" to={`/room/${r.id}`}>
              <RoomIcon room={r} className={isHeating(r) ? 'hot' : ''} />
              <span className="nr-name">
                <b>{roomTitle(r)}</b>
                <span className="muted small">{stateLine(r).text}</span>
              </span>
              <span className="nr-temp">{fmtC(num(r.temperature))}°</span>
            </Link>
          ))}
        </section>
        <div className={`pipe ${anyHeat ? 'on' : ''}`} aria-hidden />
        <section className="node">
          <div className="node-h">Circuitos de cada habitación</div>
          {rooms.map(({ r, zones }) => {
            const on = zones.some((z) => onoff(z.m))
            return (
              <div key={r.id} className={`zrow ${on ? 'on' : ''}`}>
                <span className="zled" />
                <span className="zr-name">
                  <b>{roomTitle(r)}</b>
                  <span className="muted small">
                    {zones.length
                      ? `zona${zones.length > 1 ? 's' : ''} ${zones.map((z) => z.z).join(zones.length > 1 ? ' y ' : '')}`
                      : 'sin zona asignada'}
                  </span>
                </span>
                <span className="zr-state">{on ? 'Pasa agua caliente' : 'Sin paso de agua'}</span>
              </div>
            )
          })}
          <p className="muted small node-note">
            Cada zona es una válvula de la centralita que deja pasar el agua caliente al suelo o
            radiador de una habitación. La salida A mueve la bomba
            {freeZones.length
              ? `; ${freeZones.length > 1 ? 'las zonas' : 'la zona'} ${freeZones.map((m) => m.label.slice(-1)).join(' y ')} no ${freeZones.length > 1 ? 'tienen' : 'tiene'} habitación`
              : ''}
            .
          </p>
        </section>
        <div className={`pipe ${onoff(pumpOut) ? 'on' : ''}`} aria-hidden />
        <section className="node node-sm">
          <div className="node-h">Bomba del circuito</div>
          <div className={`node-state ${onoff(pumpOut) ? 'on' : ''}`}>
            {onoff(pumpOut) ? 'En marcha' : 'Parada'}
          </div>
          {flow && flow.value !== null ? (
            <div className="muted small">Impulsión {es(flow.text)}</div>
          ) : null}
        </section>
        <div className={`pipe ${onoff(demand) ? 'on' : ''}`} aria-hidden />
        <section className="node node-sm">
          <div className="node-h">Caldera</div>
          <div className={`node-state ${onoff(demand) ? 'on' : ''}`}>
            {onoff(demand) ? 'Pidiendo calor' : 'Sin demanda'}
          </div>
          {outdoor && outdoor.value !== null ? (
            <div className="muted small">Exterior {es(outdoor.text)}</div>
          ) : null}
        </section>
      </div>
      <details className="card section adv">
        <summary>Detalles técnicos</summary>
        <div style={{ marginTop: 10 }}>
          {flowSet ? (
            <div className="kv">
              <span>Temperatura objetivo del agua</span>
              <b>{es(flowSet.text)}</b>
            </div>
          ) : null}
          {probes.map((m) => {
            const sr = sensorRoom(m.label, S)
            return (
              <div className="kv" key={m.label}>
                <span>
                  {sr ? 'Sensor de ' + roomTitle(sr) : 'Sonda ' + m.label}{' '}
                  <span className="muted">({m.label})</span>
                </span>
                <b>{es(m.text)}</b>
              </div>
            )
          })}
          {freeZones.length ? (
            <div className="kv">
              <span>Zonas sin habitación</span>
              <b>{freeZones.map((m) => m.label.slice(-1)).join(', ')}</b>
            </div>
          ) : null}
        </div>
      </details>
      <details className="card section adv">
        <summary>Pantalla original del equipo</summary>
        <div className="lines small">
          {(S.status_lines || []).flat().map((l, i) => (
            <p key={i}>{es(l) || '\u00a0'}</p>
          ))}
        </div>
        <Link className="link-more" to="/menu/status">
          Abrir en el menú Experto <Icon name="chevron" className="mini" />
        </Link>
      </details>
    </>
  )
}
