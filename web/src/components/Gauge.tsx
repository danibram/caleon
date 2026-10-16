import type { Room } from '../context/AppContext.tsx'
import { fmtC, num } from '../lib/format.ts'
import { isHeating, modeOf } from '../lib/rooms.ts'
import { Icon } from './Icon.tsx'

const MIN = 5
const MAX = 30
const START = 135
const SWEEP = 270
const R = 84

const MODE_COLOR: Record<string, string> = {
  turbo: 'var(--turbo)',
  eco: 'var(--eco)',
  regular: 'var(--normal)',
}

function frac(v: number) {
  return (Math.min(MAX, Math.max(MIN, v)) - MIN) / (MAX - MIN)
}

function pt(a: number, rad = R) {
  return [100 + rad * Math.cos((a * Math.PI) / 180), 100 + rad * Math.sin((a * Math.PI) / 180)] as const
}

export function Gauge({ room }: { room: Room }) {
  const mode = modeOf(room)
  const t = num(room.temperature)
  const sp = num(room.tset)
  const off = mode === 'off' || sp === null
  const [x0, y0] = pt(START)
  const [x1, y1] = pt(START + SWEEP)
  const full = `M${x0.toFixed(2)} ${y0.toFixed(2)} A${R} ${R} 0 1 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`
  const color = MODE_COLOR[mode] || 'var(--accent)'
  const pct = off ? 0 : Math.round(frac(sp) * 1000) / 10
  const sA = off ? 0 : frac(sp) * SWEEP
  const tA = t === null ? 0 : frac(t) * SWEEP
  const heating = isHeating(room)

  const ticks = [10, 15, 20, 25].map((v) => {
    const a = START + frac(v) * SWEEP
    const [ax, ay] = pt(a, R - 13)
    const [bx, by] = pt(a, R - 18)
    return (
      <line
        key={v}
        x1={ax}
        y1={ay}
        x2={bx}
        y2={by}
        stroke="var(--line)"
        strokeWidth="2"
        strokeLinecap="round"
      />
    )
  })

  let tempNeedle = null
  if (t !== null) {
    const [ia, ib] = [pt(START, R - 11), pt(START, R + 11)]
    tempNeedle = (
      <g className="g-rot temp" style={{ transform: `rotate(${tA}deg)` }}>
        <line x1={ia[0]} y1={ia[1]} x2={ib[0]} y2={ib[1]} stroke="var(--card)" strokeWidth="7" strokeLinecap="round" />
        <line x1={ia[0]} y1={ia[1]} x2={ib[0]} y2={ib[1]} stroke="var(--ink)" strokeWidth="3" strokeLinecap="round" />
      </g>
    )
  }

  return (
    <div
      className={`gauge ${off ? 'off' : ''} ${heating ? 'heating' : ''}`}
      data-pct={pct}
      data-sa={sA}
      data-ta={tA}
    >
      <svg
        viewBox="0 0 200 200"
        role="img"
        aria-label={`Temperatura ${fmtC(t)} grados${off ? ', apagada' : `, objetivo ${fmtC(sp)}`}`}
      >
        <path className="g-track" d={full} fill="none" stroke="var(--soft)" strokeWidth="14" strokeLinecap="round" />
        {!off ? (
          <path
            className="g-fill"
            d={full}
            pathLength={100}
            strokeDasharray="100 200"
            style={{ strokeDashoffset: 100 - pct }}
            fill="none"
            stroke={color}
            strokeWidth="14"
            strokeLinecap="round"
          />
        ) : null}
        {ticks}
        {!off ? (
          <g className="g-rot set" style={{ transform: `rotate(${sA}deg)` }}>
            <circle cx={x0} cy={y0} r="4.5" fill="var(--card)" />
          </g>
        ) : null}
        {tempNeedle}
        <text x={x0 - 6} y={y0 + 22} className="g-scale">
          {MIN}°
        </text>
        <text x={x1 - 8} y={y1 + 22} className="g-scale">
          {MAX}°
        </text>
      </svg>
      <div className="center">
        <div className="g-cap">
          {heating ? (
            <>
              <Icon name="flame" className="mini heat" /> Calentando
            </>
          ) : (
            'Ahora'
          )}
        </div>
        <div className="g-temp" data-vt-hero style={{ viewTransitionName: 'room-temp' }}>
          {fmtC(t)}
          <small>°</small>
        </div>
        <div className="g-set">
          {off ? (
            'Apagada'
          ) : (
            <>
              Objetivo <b data-tick={`set${room.id}`}>{fmtC(sp)} °C</b>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
