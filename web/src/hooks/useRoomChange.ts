import { useCallback } from 'react'
import { SYSTEM } from '../lib/constants.ts'
import { useApp } from '../context/AppContext.tsx'
import { roomById } from '../lib/rooms.ts'
import { errorMessage, num } from '../lib/format.ts'
import type { RoomState, Snapshot } from '../api/types.ts'

/** One change per request, as the PATCH handler applies exactly one field. */
export type RoomChangeBody =
  | { tdelta: number }
  | { mode: string }
  | { name: string }
  | { season: string }
  | { vacation: string | null }

type RoomChangeResponse = RoomState & { state?: Snapshot }

export function useRoomChange() {
  const { S, setS, api, toast, rereadSoon, setRoomBusy } = useApp()

  return useCallback(
    async (id: number, body: RoomChangeBody) => {
      const room = roomById(S, id)
      const before = room && { mode: room.mode, tset: room.tset }

      setS((prev) => {
        if (!prev) return prev
        const next = structuredClone(prev)
        const r = roomById(next, id)
        if ('mode' in body) {
          if (r) r.mode = body.mode
          if (id === SYSTEM && next.system) next.system.mode = body.mode
        } else if ('tdelta' in body && r) {
          const t = num(r.tset)
          if (t !== null) r.tset = (t + 0.5 * body.tdelta).toFixed(1) + '°C'
        }
        return next
      })
      setRoomBusy(id, true)

      try {
        const st = await api<RoomChangeResponse>(`/api/rooms/${id}`, { body, any: true })
        const { state: full, ...patch } = st
        setS((prev) => {
          const base = full ?? prev
          if (!base) return base
          const next = structuredClone(base)
          if (id === SYSTEM) next.system = patch
          else {
            const r = roomById(next, id)
            if (r) Object.assign(r, patch)
          }
          return next
        })
        rereadSoon()
        if (navigator.vibrate) navigator.vibrate(8)
      } catch (e) {
        if (room && before) {
          setS((prev) => {
            if (!prev) return prev
            const next = structuredClone(prev)
            const r = roomById(next, id)
            if (r) Object.assign(r, before)
            return next
          })
        }
        toast('No se pudo aplicar: ' + errorMessage(e), 6000)
      }

      setRoomBusy(id, false)
    },
    [S, setS, api, toast, rereadSoon, setRoomBusy],
  )
}

export function useRoomControlGuard() {
  const { staleData, toast, roomBusy } = useApp()
  return {
    staleData,
    roomBusy,
    blockedToast: () => toast('No se aplicó · el termostato no responde ahora mismo'),
  }
}
