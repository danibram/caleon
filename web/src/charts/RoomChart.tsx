import { useEffect, useEffectEvent, useRef, type CSSProperties } from 'react'
import type { StateSnapshot } from '../api/types.ts'
import { drawRoomChart } from './chartData.ts'

interface RoomChartProps {
  hours: number
  state: StateSnapshot
  onlyRoom?: string | null
  what?: 'temp' | 'hum'
  svgId: string
  className?: string
  style?: CSSProperties
}

export function RoomChart({
  hours,
  state,
  onlyRoom = null,
  what = 'temp',
  svgId,
  className = 'chart',
  style,
}: RoomChartProps) {
  const svgRef = useRef<SVGSVGElement>(null)
  const legendRef = useRef<HTMLDivElement>(null)
  // redraw for a new reading from the thermostat, not for optimistic edits of the snapshot
  const updated = state.updated

  const draw = useEffectEvent(() => {
    const svg = svgRef.current
    const legend = legendRef.current
    if (svg && legend) void drawRoomChart({ svg, legend, hours, state, onlyRoom, what })
  })

  useEffect(() => {
    draw()
    const id = window.setInterval(draw, 60000)
    return () => clearInterval(id)
  }, [hours, onlyRoom, what, updated])

  return (
    <>
      <div className="chart-wrap">
        <svg ref={svgRef} className={className} id={svgId} style={style} />
      </div>
      <div className="legend" ref={legendRef} id={svgId.replace(/^c(\d)$/, 'l$1').replace(/Chart$/, 'Legend')} />
    </>
  )
}
