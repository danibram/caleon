import type { Room } from '../context/AppContext'

export function RoomIcon({
  room,
  className = '',
  size = '',
}: {
  room: Room
  className?: string
  size?: 'sm' | 'lg' | ''
}) {
  const type = room.type || 'room01'
  return (
    <span
      className={`ricon ${className} ${size === 'lg' ? 'lg' : ''}`}
      style={{ ['--src' as string]: `url('/icons/${type}.svg?mask=1')` }}
      aria-hidden
    >
      <i />
    </span>
  )
}
