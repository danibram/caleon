import { useCallback } from 'react'
import { SYSTEM } from '../lib/constants.ts'
import { useApp } from '../context/AppContext.tsx'
import { roomById } from '../lib/rooms.ts'
import { num } from '../lib/format.ts'
import type { RoomState, Snapshot } from '../api/types.ts'

type RoomChangeBody = {
  tdelta?: number
  mode?: string
  name?: string
  season?: string
  vacation?: string | null
}

type RoomChangeResponse = RoomState & { state?: Snapshot }

export function useRoomChange(rerender?: () => void) {
  const { S, setS, api, toast, rereadSoon, navGen, setRoomBusy, refreshView } = useApp()

  return useCallback(
    async (id: number | string, body: RoomChangeBody) => {
      const seq = navGen.current
      const redraw = () => {
        if (seq === navGen.current) rerender?.()
      }
      const room = roomById(S, id)
      const before = room && { mode: room.mode, tset: room.tset }
      const numId = Number(id)

      setS((prev) => {
        if (!prev) return prev
        const next = structuredClone(prev)
        const r = roomById(next, id)
        if (r && body.mode) r.mode = body.mode
        if (r && body.tdelta) {
          const t = num(r.tset)
          if (t !== null) r.tset = (t + 0.5 * body.tdelta).toFixed(1) + '°C'
        }
        if (numId === SYSTEM && body.mode && next.system) next.system.mode = body.mode
        return next
      })
      setRoomBusy(numId, true)
      redraw()

      try {
        const st = await api<RoomChangeResponse>(`/api/rooms/${id}`, { body, any: true })
        const { state: full, ...patch } = st
        setS((prev) => {
          const base = full ?? prev
          if (!base) return base
          const next = structuredClone(base)
          if (numId === SYSTEM) next.system = patch
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
        toast('No se pudo aplicar: ' + (e instanceof Error ? e.message : String(e)), 6000)
      }

      setRoomBusy(numId, false)
      if (seq !== navGen.current || numId === SYSTEM) {
        refreshView()
        return
      }
      redraw()
    },
    [
      S,
      setS,
      api,
      toast,
      rereadSoon,
      navGen,
      setRoomBusy,
      rerender,
      refreshView,
    ],
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
