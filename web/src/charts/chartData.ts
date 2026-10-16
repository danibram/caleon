import { api } from '../api/client.ts'
import type { HistoryResponse, MetricsResponse, StateSnapshot } from '../api/types.ts'
import { modeOf, roomTitle } from '../lib/rooms.ts'
import { deblip, lineChart, onIntervals, type ChartSeries } from './lineChart.ts'

export const PROBES = [
  'oklch(0.6 0.16 345)',
  'oklch(0.58 0.1 120)',
  'oklch(0.55 0.05 260)',
  'oklch(0.6 0.09 60)',
  'oklch(0.6 0.08 200)',
  'oklch(0.55 0.12 290)',
]

export function palette(): string[] {
  return getComputedStyle(document.documentElement)
    .getPropertyValue('--series')
    .split(',')
    .map((s) => s.trim())
}

const chartTokens = new WeakMap<SVGSVGElement, number>()

export function historySpan(state: StateSnapshot): number {
  return state.history_since ? Date.now() / 1000 - state.history_since : 0
}

export function bestRange(ranges: number[], want: number, state: StateSnapshot): number {
  const span = historySpan(state)
  return (
    [...ranges].reverse().find((h) => h <= want && (h === ranges[0] || span >= h * 3600 * 0.3)) ||
    ranges[0]!
  )
}

export async function drawRoomChart(
  svg: SVGSVGElement,
  legend: HTMLElement,
  hours: number,
  state: StateSnapshot,
  onlyRoom: string | null,
  what: 'temp' | 'hum' = 'temp',
): Promise<void> {
  const tok = (chartTokens.get(svg) || 0) + 1
  chartTokens.set(svg, tok)
  let data: HistoryResponse
  try {
    data = await api<HistoryResponse>(`/api/history?hours=${hours}`)
  } catch {
    return
  }
  if (chartTokens.get(svg) !== tok) return
  const pal = palette(),
    series: ChartSeries[] = []
  let bands: [number, number][] = []
  Object.entries(data).forEach(([room, rows]) => {
    if (onlyRoom !== null && room !== onlyRoom) return
    const r = state.rooms.find((r) => String(r.id) === room),
      name = r ? roomTitle(r) : 'Sala ' + room,
      c = pal[Number(room) % pal.length]!
    if (what === 'hum') {
      if (rows.some((x) => x[2] !== null))
        series.push({
          name,
          color: c,
          step: true,
          points: deblip(
            rows.map((x) => [x[0], x[2]] as [number, number | null]),
            0.6,
          ),
        })
      return
    }
    const main: ChartSeries = {
      name,
      color: c,
      step: true,
      points: deblip(rows.map((x) => [x[0], x[1]] as [number, number | null])),
    }
    series.push(main)
    if (r && modeOf(r) !== 'off') {
      const tset: ChartSeries = {
        name: onlyRoom === null ? name + ' · objetivo' : 'Objetivo',
        color: c,
        dashed: true,
        step: true,
        noLegend: onlyRoom === null,
        points: rows.map((x) => [x[0], x[3]] as [number, number | null]),
      }
      if (onlyRoom === null) {
        main.twins = [tset]
        tset.twinOf = main
      }
      series.push(tset)
    }
  })
  if (onlyRoom !== null && what === 'temp') {
    const r = state.rooms.find((r) => String(r.id) === onlyRoom)
    const keys = (r?.outputs || []).map((o) => `Salidas · ${o.label}`)
    if (keys.length) {
      try {
        bands = onIntervals(await api<MetricsResponse>(`/api/metrics?hours=${hours}`), keys)
      } catch {
        /* ignore */
      }
    }
    if (chartTokens.get(svg) !== tok) return
  }
  lineChart(svg, legend, series, hours, {
    yUnit: what === 'hum' ? '%' : '°',
    bands,
  })
}
