import { useEffect, useRef, type CSSProperties } from 'react'
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
  const stateRef = useRef(state)
  stateRef.current = state
  // redraw for a new reading from the thermostat, not for optimistic edits of the snapshot
  const updated = state.updated

  useEffect(() => {
    const svg = svgRef.current
    const legend = legendRef.current
    if (!svg || !legend) return
    const run = () => void drawRoomChart(svg, legend, hours, stateRef.current, onlyRoom, what)
    run()
    const id = window.setInterval(run, 60000)
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
