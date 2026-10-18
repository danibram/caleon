import type { Room } from '../api/types.ts'
import { useRoomChange, useRoomControlGuard } from '../hooks/useRoomChange.ts'
import { MODE_LABEL } from '../lib/constants.ts'
import { modeOf } from '../lib/rooms.ts'
import { flatScheduleToday, scheduledModeNow } from '../lib/schedule.ts'

/** A mode chosen by hand stays until the program's next change, so a room can sit in Off while its
 *  program says Normal; with a program that never changes it would stay there for good. */
export function FollowSchedule({ room }: { room: Room }) {
  const roomChange = useRoomChange()
  const { staleData, roomBusy, blockedToast } = useRoomControlGuard()
  const scheduled = scheduledModeNow(room)
  const mode = modeOf(room)
  if (!scheduled || scheduled === mode || room.vacation) return null
  const flat = flatScheduleToday(room)
  return (
    <div className="follow">
      <span className="muted small">
        Según el horario ahora tocaría <b>{MODE_LABEL[scheduled] || scheduled}</b>.{' '}
        {flat
          ? 'Como el horario de hoy no cambia de modo, el termostato no volverá a él por sí solo.'
          : 'El modo elegido a mano dura hasta el siguiente cambio del horario.'}
      </span>
      <button
        type="button"
        className="btn"
        disabled={roomBusy.has(room.id)}
        onClick={() => {
          if (staleData) {
            blockedToast()
            return
          }
          void roomChange(room.id, { mode: scheduled })
        }}
      >
        Seguir el horario
      </button>
    </div>
  )
}
