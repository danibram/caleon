import type { Room, Snapshot } from '../context/AppContext.tsx'
import { Icon } from './Icon.tsx'
import { bandCaption, modeWithTemp, nowSlotIdx, segments, todayIdx } from '../lib/schedule.ts'

interface BandProps {
  room: Room
  S: Snapshot | null
  big?: boolean
}

export function Band({ room, S, big = false }: BandProps) {
  if (!room.week) return null
  const slots = room.week[todayIdx()]!
  const ns = nowSlotIdx()
  const { caption, paused } = bandCaption(room, big)
  const uniqueModes = [...new Set(slots)]

  return (
    <div className={`band ${paused ? 'paused' : ''} ${big ? 'big' : ''}`}>
      <div className="band-bar" role="img" aria-label="Programa de hoy">
        {segments(slots).map((g) => (
          <i key={`${g.start}-${g.mode}`} className={`slot-${g.mode}`} style={{ flex: g.end - g.start }} />
        ))}
        <b className="needle" style={{ left: `${((ns + 0.5) / 48) * 100}%` }} />
      </div>
      {big ? (
        <>
          <div className="band-axis">
            <span>0</span>
            <span>6</span>
            <span>12</span>
            <span>18</span>
            <span>24 h</span>
          </div>
          <div className="band-legend">
            {uniqueModes.map((m) => (
              <span key={m}>
                <i className={`sw slot-${m}`} />
                {modeWithTemp(room, m, S)}
              </span>
            ))}
          </div>
        </>
      ) : null}
      <div className="band-cap">
        <Icon name="clock" className="mini" />
        <span>{caption}</span>
      </div>
    </div>
  )
}
