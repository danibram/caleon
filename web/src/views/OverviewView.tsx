import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router'
import { api } from '../api/client.ts'
import type { HistoryResponse } from '../api/types.ts'
import { RoomChart } from '../charts/RoomChart.tsx'
import { historySpan } from '../charts/chartData.ts'
import { Banner } from '../components/Banner.tsx'
import { Icon } from '../components/Icon.tsx'
import { RoomCard } from '../components/RoomCard.tsx'
import { RoomControls } from '../components/RoomControls.tsx'
import { useApp } from '../context/AppContext.tsx'
import { SYSTEM } from '../lib/constants.ts'
import { MODE_LABEL } from '../lib/constants.ts'
import { fmtC, num } from '../lib/format.ts'
import { insights, upcomingChangeToday } from '../lib/insights.ts'
import { hhmm } from '../lib/schedule.ts'
import { isHeating, modeOf, roomById, roomTitle } from '../lib/rooms.ts'

const history6h = () => api<HistoryResponse>('/api/history?hours=6')

function HomeSummary({ hist }: { hist: HistoryResponse | null }) {
  const { S } = useApp()
  if (!S) return null

  const rooms = S.rooms.filter((r) => r.type !== 'outdoor')
  const temps = rooms.map((r) => num(r.temperature)).filter((v): v is number => v !== null)
  const avg = temps.length ? temps.reduce((a, b) => a + b, 0) / temps.length : null
  const heating = rooms.filter(isHeating)
  const off = rooms.filter((r) => modeOf(r) === 'off')
  const hum = rooms.map((r) => num(r.humidity)).filter((v): v is number => v !== null)
  const outdoor = (S.status || []).find((m) => m.label === 'Exterior')
  const ins = insights(hist, S)
  const fmtClock = (ts: number) =>
    new Date(ts * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })

  let line: string
  if (heating.length) {
    line = `Calentando ${heating.map((r) => roomTitle(r)).join(' y ')}`
  } else if (off.length === rooms.length) {
    line = ins.offSince ? `Todo apagado desde las ${fmtClock(ins.offSince)}` : 'Calefacción apagada en toda la casa'
  } else {
    line = 'Todas las habitaciones en su temperatura'
  }

  const trend =
    ins.trend === undefined
      ? ''
      : Math.abs(ins.trend) < 0.05
        ? ' · estable'
        : ` · ${ins.trend > 0 ? 'sube' : 'baja'} ${fmtC(Math.abs(ins.trend))} °C/h`

  const upcoming = upcomingChangeToday(rooms)

  return (
    <section className="hero">
      <div className="hero-main">
        <div className="hero-kicker">
          {heating.length ? <Icon name="flame" className="hero-ic heat" /> : null}
          {line}
        </div>
        <div className="hero-temp">
          {fmtC(avg)}
          <small>°</small>
        </div>
        <div className="hero-sub">
          media de la casa
          {temps.length > 1
            ? ` · de ${fmtC(Math.min(...temps))} a ${fmtC(Math.max(...temps))} °C`
            : ''}
          {trend}
        </div>
      </div>
      <div className="hero-facts">
        {hum.length ? (
          <div>
            <span>{Math.round(hum.reduce((a, b) => a + b, 0) / hum.length)} %</span>
            humedad
          </div>
        ) : null}
        {outdoor && outdoor.value != null ? (
          <div>
            <span>{fmtC(outdoor.value)}°</span>
            fuera
          </div>
        ) : null}
        {upcoming ? (
          <div>
            <span>{hhmm(upcoming.nc.slot)}</span>
            {roomTitle(upcoming.r)} a {(MODE_LABEL[upcoming.nc.mode] || '').toLowerCase()}
          </div>
        ) : (
          <div>
            <span>
              {rooms.length - off.length}/{rooms.length}
            </span>
            encendidas
          </div>
        )}
      </div>
    </section>
  )
}

function AllRoomsCard({ mobileOnly = false }: { mobileOnly?: boolean }) {
  const { S } = useApp()
  const sys = S ? roomById(S, SYSTEM) : undefined
  if (!sys || !S) return null

  const cls = mobileOnly ? 'card section all-rooms mobile-only' : 'card all-rooms'

  const title = (
    <div className="row">
      <span className="ic-circle">
        <Icon name="home" />
      </span>
      <div>
        <div className="room-title">Toda la casa</div>
        <div className="muted small">Un modo para todas las habitaciones</div>
      </div>
    </div>
  )

  return (
    <div className={cls} data-room={SYSTEM}>
      {mobileOnly ? <div className="spread">{title}</div> : title}
      <div style={{ marginTop: 12 }}>
        <RoomControls room={sys} S={S} compact="modes" />
      </div>
    </div>
  )
}

export function OverviewView() {
  const { S } = useApp()
  // the last good history stays on screen while a refetch fails
  const { data: hist = null } = useQuery({
    queryKey: ['history', 6],
    queryFn: history6h,
    staleTime: 60000,
    refetchInterval: 60000,
    retry: false,
  })

  if (!S) return null

  const chartHours = Math.min(24, Math.max(3, (historySpan(S) / 3600) * 1.15))

  return (
    <>
      <Banner />
      <div className="home-top">
        <HomeSummary hist={hist} />
        <AllRoomsCard />
      </div>
      <div className="spread section-head">
        <h2>Habitaciones</h2>
      </div>
      <div className="grid stagger">
        {S.rooms.map((r, i) => (
          <RoomCard key={r.id} room={r} S={S} index={i} />
        ))}
      </div>
      <AllRoomsCard mobileOnly />
      <div className="card section chart-card">
        <div className="chart-head">
          <b>Temperatura de hoy</b>
          <Link className="link-more" to="/charts">
            Todas las gráficas <Icon name="chevron" className="mini" />
          </Link>
        </div>
        <RoomChart hours={chartHours} state={S} onlyRoom={null} svgId="ovChart" />
      </div>
    </>
  )
}
