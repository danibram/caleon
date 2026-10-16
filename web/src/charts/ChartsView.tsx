import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { api } from '../api/client.ts'
import type { MetricsResponse, StateSnapshot } from '../api/types.ts'
import { useApp } from '../context/AppContext.tsx'
import { metricName } from '../lib/metrics.ts'
import { modeOf, roomTitle } from '../lib/rooms.ts'
import { bestRange, PROBES } from './chartData.ts'
import { lineChart } from './lineChart.ts'
import { RangeSeg } from './RangeSeg.tsx'
import { RoomChart } from './RoomChart.tsx'

export function ChartsView() {
  const { S } = useApp()
  if (!S) return null
  return <Charts state={S} />
}

function Charts({ state }: { state: StateSnapshot }) {
  const [hours, setHours] = useState(() => bestRange([6, 24, 168, 720], 24, state))
  const drawTok = useRef(0)

  const c3SvgRef = useRef<SVGSVGElement>(null)
  const c3LegendRef = useRef<HTMLDivElement>(null)
  const c4SvgRef = useRef<SVGSVGElement>(null)
  const c4LegendRef = useRef<HTMLDivElement>(null)
  const c3CardRef = useRef<HTMLDivElement>(null)
  const c4CardRef = useRef<HTMLDivElement>(null)

  const outCount = (state.status || []).filter((m) => m.group === 'Salidas' && metricName(m, state)).length

  const humRooms = state.rooms.filter((r) => r.humidity && r.humidity !== '--')
  const humSuffix = humRooms.length === 1 ? ' · ' + roomTitle(humRooms[0]!) : ''

  // reads the snapshot at call time: the metric charts redraw on the timer, not on every state refresh
  const drawMetrics = useEffectEvent(async () => {
    const tok = ++drawTok.current
    let m: MetricsResponse
    try {
      m = await api<MetricsResponse>(`/api/metrics?hours=${hours}`)
    } catch {
      return
    }
    if (tok !== drawTok.current) return

    const c4Svg = c4SvgRef.current
    const c4Legend = c4LegendRef.current
    const c3Svg = c3SvgRef.current
    const c3Legend = c3LegendRef.current
    if (!c4Svg || !c4Legend || !c3Svg || !c3Legend) return

    const keys = Object.keys(m)
    const names = new Map(keys.map((k) => [k, metricName(k, state) || '']))
    const analog = keys.filter(
      (k) =>
        !k.startsWith('Salidas') &&
        !/· Modo$/.test(k) &&
        names.get(k) &&
        !/^(Sensor|Humedad) de /.test(names.get(k)!) &&
        (m[k] || []).some((p) => p[1] !== null),
    )
    const outputs = keys
      .filter((k) => k.startsWith('Salidas') && names.get(k))
      .sort((a, b) => names.get(a)!.localeCompare(names.get(b)!, 'es'))

    lineChart(
      c4Svg,
      c4Legend,
      outputs.map((k) => ({
        name: names.get(k)!,
        color: /Bomba|caldera/.test(names.get(k)!) ? 'var(--cool)' : 'var(--accent)',
        points: m[k]!,
        noLegend: true,
      })),
      hours,
      { lanes: true },
    )
    const c4Card = c4CardRef.current
    if (c4Card && c4Svg.style.display === 'none') c4Card.classList.add('collapsed')

    const varies = (k: string) => {
      const v = (m[k] || []).map((p) => p[1]).filter((x) => x !== null) as number[]
      return v.length > 1 && Math.max(...v) - Math.min(...v) > 0.05
    }
    const shownAnalog = analog.filter(varies)
    const c3Card = c3CardRef.current
    if (c3Card) c3Card.hidden = !shownAnalog.length

    lineChart(
      c3Svg,
      c3Legend,
      shownAnalog.map((k, i) => ({
        name: names.get(k)!,
        color: PROBES[i % PROBES.length]!,
        points: m[k]!,
        hidden: false,
      })),
      hours,
      { yUnit: '°' },
    )
  })

  useEffect(() => {
    void drawMetrics()
    const id = window.setInterval(() => {
      void drawMetrics()
    }, 60000)
    return () => clearInterval(id)
  }, [hours])

  const laneHeight = Math.max(120, 34 * outCount + 36)

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

      <div className="card chart-card">
        <div className="chart-head">
          <b>Temperatura</b>
          {state.rooms.some((r) => modeOf(r) !== 'off') ? (
            <span className="muted small">Discontinua: objetivo</span>
          ) : null}
        </div>
        <RoomChart hours={hours} state={state} svgId="c1" what="temp" />
      </div>

      <div className="card chart-card">
        <div className="chart-head">
          <b>Humedad{humSuffix}</b>
        </div>
        <RoomChart hours={hours} state={state} svgId="c2" what="hum" style={{ height: 180 }} />
      </div>

      <div className="card chart-card" ref={c4CardRef}>
        <div className="chart-head">
          <b>Cuándo ha calentado cada habitación</b>
          <span className="muted small">Según las válvulas de cada zona</span>
        </div>
        <div className="chart-wrap">
          <svg ref={c4SvgRef} className="chart" id="c4" style={{ height: laneHeight }} />
        </div>
        <div className="legend" ref={c4LegendRef} id="l4" />
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
