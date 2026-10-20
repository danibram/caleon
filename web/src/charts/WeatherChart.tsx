import { useEffect, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client.ts'
import type { WeatherPoint } from '../api/generated/WeatherPoint.ts'
import type { WeatherSource } from '../api/generated/WeatherSource.ts'
import type { WeatherView } from '../api/generated/WeatherView.ts'
import { weatherProviderName } from '../lib/weather.ts'
import { panelArgs, type PanelOptions } from './chartData.ts'
import { lineChart, unsync, type ChartSeries } from './lineChart.ts'

const kindName = (kind: WeatherSource['kind']) =>
  kind === 'observation' ? 'observación' : kind === 'reanalysis' ? 'reanálisis' : 'estimación'

// an hour more than the window, so the hourly line enters it from the left edge
const historyQuery = (hours: number) => ({
  queryKey: ['weather-history', hours],
  queryFn: () => api<WeatherPoint[]>(`/api/weather/history?hours=${hours + 1}`),
  refetchInterval: 15000,
})

/** Outside temperature as a panel of the stacked chart on Gráficas. */
export function WeatherChart({
  view,
  hours,
  timeRange,
  panel,
}: {
  view: WeatherView
  hours: number
  timeRange: [number, number]
  panel?: PanelOptions
}) {
  const svg = useRef<SVGSVGElement>(null)
  const legend = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  const { data, error: loadError } = useQuery(historyQuery(hours))
  // a provider error only matters when the chart has nothing recent to show
  const recent = data?.some(({ sample }) => sample.ts > timeRange[1] - 3 * 3600) ?? false

  useEffect(() => {
    const el = svg.current
    if (!el) return
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(Math.round(entry.contentRect.width))
    })
    observer.observe(el)
    return () => {
      observer.disconnect()
      unsync(el)
    }
  }, [])

  useEffect(() => {
    if (!svg.current || !legend.current || !data) return
    // one line per kind of source: a provider's periods join into the same line, its gaps stay gaps
    const series = new Map<string, ChartSeries>()
    for (const { source, sample } of data) {
      const name = `${weatherProviderName(source.provider)}${source.station_id ? ` · ${source.station_id}` : ''} · ${kindName(source.kind)}`
      let line = series.get(name)
      if (!line) {
        line = {
          name,
          short: 'Exterior',
          color: 'var(--ink)',
          gapSeconds: source.resolution_seconds * 1.5,
          points: [],
        }
        series.set(name, line)
      }
      line.points.push([sample.ts, sample.temperature_c])
    }
    for (const line of series.values()) line.points.sort((a, b) => a[0] - b[0])
    lineChart({
      svg: svg.current,
      legend: legend.current,
      series: [...series.values()],
      hours,
      yUnit: '°',
      timeRange,
      ...panelArgs(panel),
      emptyMessage: 'Todavía no hay temperatura exterior en este periodo. La cobertura depende del proveedor.',
    })
  }, [data, hours, timeRange, width, panel])

  return (
    <section className="chart-panel" aria-label="Temperatura exterior">
      {view.status.backup_since ? (
        <p className="banner" role="status">
          AEMET no responde. Mientras tanto se muestra Open-Meteo.
        </p>
      ) : null}
      {!view.history_enabled ? (
        <p className="banner">El servidor tiene el histórico deshabilitado. No se recogen temperaturas exteriores.</p>
      ) : null}
      {loadError ? (
        <p className="banner" role="alert">
          No se pudo leer el histórico exterior.
        </p>
      ) : view.status.error && !recent ? (
        <p className="muted small" role="status">
          {view.status.error}
        </p>
      ) : null}
      <p className="chart-panel-label">Exterior · °C</p>
      <div className="chart-wrap">
        <svg ref={svg} className="chart" id="weatherChart" style={{ height: 110 }} />
      </div>
      <div className="legend" ref={legend} />
    </section>
  )
}

/** Where the outside temperature comes from, for the foot of the stacked chart. */
export function WeatherFoot({ view, hours }: { view: WeatherView; hours: number }) {
  const { data } = useQuery(historyQuery(hours))
  const sources = [
    ...new Set(data?.map(({ source }) => `${weatherProviderName(source.provider)} (${kindName(source.kind)})`) ?? []),
  ]
  const last = view.status.last_sample_at
  return (
    <p className="chart-foot muted small">
        Exterior: {sources.length ? `${sources.join(', ')}, datos horarios` : 'sin datos todavía'}
        {last
          ? ` · último dato ${new Date(last * 1000).toLocaleString('es-ES', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}`
          : ''}
        {data?.some((point) => point.source.provider === 'open-meteo') ? (
          <>
            {' · '}
            <a href="https://open-meteo.com/" target="_blank" rel="noopener noreferrer">
              Open-Meteo
            </a>{' '}
            <a href="https://creativecommons.org/licenses/by/4.0/" target="_blank" rel="noopener noreferrer">
              CC BY 4.0
            </a>
          </>
        ) : null}
        {data?.some((point) => point.source.provider === 'aemet') ? (
          <>
            {' · '}
            <a href="https://www.aemet.es/es/nota_legal" target="_blank" rel="noopener noreferrer">
              AEMET
            </a>
          </>
        ) : null}
      </p>
  )
}
