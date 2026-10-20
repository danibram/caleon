import { useEffect, useEffectEvent, useMemo, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { weatherQuery } from '../lib/weather.ts'
import { WeatherChart, WeatherFoot } from './WeatherChart.tsx'
import { api } from '../api/client.ts'
import type { MetricPoint, MetricsResponse, StateSnapshot } from '../api/types.ts'
import { useApp } from '../context/AppContext.tsx'
import { isPresent } from '../lib/guards.ts'
import { metricName } from '../lib/metrics.ts'
import { roomTitle } from '../lib/rooms.ts'
import { bestRange, historySpan, panelArgs, PROBES, type PanelOptions } from './chartData.ts'
import { lineChart, unsync } from './lineChart.ts'
import { RangeSeg } from './RangeSeg.tsx'
import { RoomChart } from './RoomChart.tsx'

/** every chart on this page shares one time axis and one hover cursor */
const SYNC = 'charts'
const SYNC_ONLY: PanelOptions = { sync: SYNC }

export function ChartsView() {
  const { S } = useApp()
  if (!S) return null
  return <Charts state={S} />
}

function Charts({ state }: { state: StateSnapshot }) {
  const [hours, setHours] = useState(() => bestRange({ ranges: [6, 24, 168, 720], want: 24, state }))
  const drawTok = useRef(0)
  const metrics = useRef<MetricsResponse | null>(null)
  const { data: weather } = useQuery(weatherQuery)
  const timeRange = useMemo<[number, number]>(() => {
    const end = state.updated ?? 0
    return [end - hours * 3600, end]
  }, [hours, state.updated])

  const c3SvgRef = useRef<SVGSVGElement>(null)
  const c3LegendRef = useRef<HTMLDivElement>(null)
  const c4SvgRef = useRef<SVGSVGElement>(null)
  const c4LegendRef = useRef<HTMLDivElement>(null)
  const c3CardRef = useRef<HTMLDivElement>(null)
  const c4CardRef = useRef<HTMLDivElement>(null)
  const roomsReadout = useRef<HTMLDivElement>(null)
  const heatReadout = useRef<HTMLDivElement>(null)
  const outReadout = useRef<HTMLDivElement>(null)
  const roomsLegend = useRef<HTMLDivElement>(null)
  const weatherOn = weather?.settings.enabled ?? false
  // the stacked figure: one readout block on top, the time axis only under the last panel
  const tip = useRef<HTMLDivElement>(null)
  const roomsPanel = useMemo<PanelOptions>(
    () => ({ sync: SYNC, readout: roomsReadout, axis: false, floating: tip }),
    [],
  )
  const heatPanel = useMemo<PanelOptions>(
    () => ({ sync: SYNC, readout: heatReadout, showTime: false, laneNames: false }),
    [],
  )
  const outPanel = useMemo<PanelOptions>(
    () => ({ sync: SYNC, readout: outReadout, showTime: false, axis: false }),
    [],
  )


  const humRooms = state.rooms.filter((r) => r.humidity && r.humidity !== '--')
  const humSuffix = humRooms.length === 1 ? ' · ' + roomTitle(humRooms[0]) : ''

  const drawMetrics = useEffectEvent(() => {
    const m = metrics.current
    if (!m) return

    const c4Svg = c4SvgRef.current
    const c4Legend = c4LegendRef.current
    const c3Svg = c3SvgRef.current
    const c3Legend = c3LegendRef.current
    if (!c4Svg || !c4Legend || !c3Svg || !c3Legend) return

    // metrics without a human name are hidden
    const named = Object.keys(m).flatMap((key) => {
      const name = metricName(key, state)
      return name ? [{ key, name, points: m[key] || [] }] : []
    })
    const analog = named.filter(
      ({ key, name, points }) =>
        !key.startsWith('Salidas') &&
        !/· Modo$/.test(key) &&
        !/^(Sensor|Humedad) de /.test(name) &&
        points.some((p) => p[1] !== null),
    )
    const outputs = named.filter(({ key }) => key.startsWith('Salidas'))
    const pump = outputs.find(({ name }) => /Bomba/.test(name))
    const demand = outputs.find(({ name }) => /caldera/i.test(name))
    const plant = [
      ...(pump ? [{ ...pump, label: 'Bomba' }] : []),
      ...(demand ? [{ ...demand, label: 'Caldera' }] : []),
    ]

    lineChart({
      svg: c4Svg,
      legend: c4Legend,
      series: plant.map(({ points, label }) => ({
        name: label,
        readoutName: label,
        color: 'color-mix(in oklch, var(--muted) 70%, var(--card))',
        points,
        noLegend: true,
      })),
      hours,
      lanes: true,
      timeRange,
      ...panelArgs(heatPanel),
    })

    const varies = (points: MetricPoint[]) => {
      const v = points.map((p) => p[1]).filter(isPresent)
      return v.length > 1 && Math.max(...v) - Math.min(...v) > 0.05
    }
    const shownAnalog = analog.filter(({ points }) => varies(points))
    const c3Card = c3CardRef.current
    if (c3Card) c3Card.hidden = !shownAnalog.length

    lineChart({
      svg: c3Svg,
      legend: c3Legend,
      series: shownAnalog.map(({ name, points }, i) => ({
        name,
        color: PROBES[i % PROBES.length],
        points,
        hidden: false,
      })),
      hours,
      yUnit: '°',
      timeRange,
      sync: SYNC,
    })
  })


  // fetches on the timer; a new snapshot only moves the time axis, so the cursor stays aligned with the room charts
  const fetchMetrics = useEffectEvent(async () => {
    const tok = ++drawTok.current
    try {
      const m = await api<MetricsResponse>(`/api/metrics?hours=${hours}`)
      if (tok !== drawTok.current) return
      metrics.current = m
      drawMetrics()
    } catch {
      /* keep the last drawing */
    }
  })
  useEffect(() => {
    metrics.current = null
    void fetchMetrics()
    const id = window.setInterval(() => {
      void fetchMetrics()
    }, 60000)
    return () => clearInterval(id)
  }, [hours])

  useEffect(() => {
    drawMetrics()
  }, [timeRange, heatPanel])

  useEffect(() => {
    const c3 = c3SvgRef.current
    const c4 = c4SvgRef.current
    return () => {
      if (c3) unsync(c3)
      if (c4) unsync(c4)
    }
  }, [])

  useEffect(() => {
    const card = c4CardRef.current
    if (!card) return
    let width = card.clientWidth
    const observer = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0 && Math.abs(entry.contentRect.width - width) > 1) {
        width = entry.contentRect.width
        drawMetrics()
      }
    })
    observer.observe(card)
    return () => observer.disconnect()
  }, [])

  const laneHeight = 12 * 2 + 4 + 24

  return (
    <>
      <div className="page-head">
        <h3>Gráficas</h3>
        <RangeSeg
          id="cRange"
          ranges={[6, 24, 168, 720]}
          current={hours}
          state={state}
          onChange={setHours}
        />
      </div>
      {historySpan(state) < 168 * 3600 * 0.3 ? (
        <p className="muted small range-note">
          Hay {Math.max(1, Math.round(historySpan(state) / 3600))} h de historial. 7 y 30 días se activan con más.
        </p>
      ) : null}

      <section className="card chart-card stacked" ref={c4CardRef} aria-label="Temperatura, calefacción y exterior">
        <div className="chart-head">
          <b>Temperatura, calefacción y exterior</b>
          <div className="legend" ref={roomsLegend} id="l1" />
        </div>
        <div className="chart-tip" ref={tip} hidden>
          <div ref={roomsReadout} />
          {weatherOn ? <div ref={outReadout} /> : null}
          <div ref={heatReadout} />
        </div>
        <p className="chart-panel-label">Interior · °C</p>
        <RoomChart
          hours={hours}
          state={state}
          svgId="c1"
          what="temp"
          style={{ height: 180 }}
          timeRange={timeRange}
          panel={roomsPanel}
          legend={roomsLegend}
          heat
        />
        {weather && weatherOn ? (
          <WeatherChart view={weather} hours={hours} timeRange={timeRange} panel={outPanel} />
        ) : null}
        <div className="chart-panel">
          <div className="chart-wrap">
            <svg ref={c4SvgRef} className="chart" id="c4" style={{ height: laneHeight }} />
          </div>
          <div className="legend" ref={c4LegendRef} id="l4" />
        </div>
        {weather && weatherOn ? <WeatherFoot view={weather} hours={hours} /> : null}
        <p className="chart-foot muted small">
          Tramo animado: la habitación está calentando. Discontinua: su objetivo. Precisión de los sensores ±1 °C.
        </p>
      </section>

      <div className="card chart-card">
        <div className="chart-head">
          <b>Humedad{humSuffix}</b>
        </div>
        <RoomChart hours={hours} state={state} svgId="c2" what="hum" style={{ height: 180 }} timeRange={timeRange} panel={SYNC_ONLY} />
      </div>

      <div className="card chart-card" ref={c3CardRef}>
        <div className="chart-head">
          <b>Otros valores del equipo</b>
        </div>
        <div className="chart-wrap">
          <svg ref={c3SvgRef} className="chart" id="c3" />
        </div>
        <div className="legend" ref={c3LegendRef} id="l3" />
      </div>
    </>
  )
}
