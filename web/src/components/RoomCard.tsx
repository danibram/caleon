import { Link } from 'react-router'
import type { Room, Snapshot } from '../context/AppContext.tsx'
import { es, fmtC, num } from '../lib/format.ts'
import { isHeating, modeOf, roomTitle, stateLine } from '../lib/rooms.ts'
import { palette } from '../charts/chartData.ts'
import { Band } from './Band.tsx'
import { Icon } from './Icon.tsx'
import { RoomIcon } from './RoomIcon.tsx'

interface RoomCardProps {
  room: Room
  S: Snapshot | null
  index?: number
}

export function RoomCard({ room, S, index = 0 }: RoomCardProps) {
  const t = num(room.temperature)
  const st = stateLine(room)
  const hum =
    room.humidity && room.humidity !== '--' ? (
      <span className="rc-hum">
        <Icon name="drop" className="mini" />
        {es(room.humidity)}
      </span>
    ) : null
  const colors = palette()
  const dotColor = colors[Number(room.id) % colors.length]

  return (
    <Link
      to={`/room/${room.id}`}
      viewTransition
      className={`card room-card ${isHeating(room) ? 'heating' : ''} ${modeOf(room) === 'off' ? 'is-off' : ''}`}
      style={{ ['--k' as string]: index }}
      data-room={room.id}
      aria-label={`${roomTitle(room)}, ${fmtC(t)} grados, ${st.text}`}
    >
      <div className="rc-top">
        <RoomIcon room={room} className={isHeating(room) ? 'hot' : ''} />
        <div className="rc-name">
          <div className="room-title">
            {roomTitle(room)}
            <i
              className="rdot"
              style={{ background: dotColor }}
              title="Color en las gráficas"
            />
          </div>
          <div className={`rc-sub st-${st.cls}`}>
            {st.cls === 'heat' ? <Icon name="flame" className="mini" /> : <span className="sdot" />}
            {st.text}
          </div>
        </div>
        <div className="rc-temp">
          <span className="temp" data-vt={room.id}>
            {fmtC(t)}
            <small>°</small>
          </span>
          {hum}
        </div>
      </div>
      {modeOf(room) === 'off' || room.vacation ? null : <Band room={room} S={S} />}
    </Link>
  )
}
