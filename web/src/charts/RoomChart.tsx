import { useEffect, useEffectEvent, useRef, type CSSProperties, type RefObject } from 'react'
import type { StateSnapshot } from '../api/types.ts'
import { drawRoomChart, type PanelOptions } from './chartData.ts'
import { unsync } from './lineChart.ts'

interface RoomChartProps {
  hours: number
  state: StateSnapshot
  onlyRoom?: string | null
  what?: 'temp' | 'hum'
  svgId: string
  className?: string
  style?: CSSProperties
  timeRange?: [number, number]
  panel?: PanelOptions
  /** an element elsewhere for the legend, for a chart stacked with others */
  legend?: RefObject<HTMLDivElement | null>
  heat?: boolean
}

export function RoomChart({
  hours,
  state,
  onlyRoom = null,
  what = 'temp',
  svgId,
  className = 'chart',
  style,
  timeRange,
  panel,
  legend: outerLegend,
  heat = false,
}: RoomChartProps) {
  const svgRef = useRef<SVGSVGElement>(null)
  const ownLegend = useRef<HTMLDivElement>(null)
  const legendRef = outerLegend ?? ownLegend
  // redraw for a new reading from the thermostat, not for optimistic edits of the snapshot
  const updated = state.updated

  const draw = useEffectEvent(() => {
    const svg = svgRef.current
    const legend = legendRef.current
    if (svg && legend) void drawRoomChart({ svg, legend, hours, state, onlyRoom, what, timeRange, panel, heat })
  })

  useEffect(() => {
    draw()
    const id = window.setInterval(draw, 60000)
    return () => clearInterval(id)
  }, [hours, onlyRoom, what, updated, timeRange])

  useEffect(() => {
    const svg = svgRef.current
    if (!svg) return
    let width = svg.clientWidth
    const observer = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0 && Math.abs(entry.contentRect.width - width) > 1) {
        width = entry.contentRect.width
        draw()
      }
    })
    observer.observe(svg)
    return () => {
      observer.disconnect()
      unsync(svg)
    }
  }, [])

  return (
    <>
      <div className="chart-wrap">
        <svg ref={svgRef} className={className} id={svgId} style={style} />
      </div>
      {outerLegend ? null : <div className="legend" ref={ownLegend} id={svgId.replace(/^c(\d)$/, 'l$1').replace(/Chart$/, 'Legend')} />}
    </>
  )
}
