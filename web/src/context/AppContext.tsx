import { createContext, useContext } from 'react'

import type { Snapshot } from '../api/types.ts'

export type { Room, RoomOutput, Snapshot, StatusMetric } from '../api/types.ts'

export type ApiOpts = {
  method?: string
  body?: unknown
  any?: boolean
}

export type AskOptions = {
  title: string
  text?: string
  value?: string
  input?: boolean
  ok?: string
  type?: string
  presets?: [string, string][]
  label?: string
  max?: number
}

export type AppContextValue = {
  S: Snapshot | null
  setS: (value: Snapshot | null | ((prev: Snapshot | null) => Snapshot | null)) => void
  reloadState: () => Promise<unknown>
  api: <T = unknown>(path: string, opts?: ApiOpts) => Promise<T>
  toast: (msg: string, ms?: number) => void
  ask: (opts: AskOptions) => Promise<string | boolean | null>
  rereadSoon: () => void
  /** Navigation generation; compare before and after an await to drop replies for a view that is gone. */
  navGen: () => number
  bumpNav: () => void
  roomBusy: Set<number>
  setRoomBusy: (id: number, busy: boolean) => void
  staleData: boolean
}

export const AppContext = createContext<AppContextValue | null>(null)

export function useAppState() {
  const ctx = useContext(AppContext)
  if (!ctx) throw new Error('useAppState must be used within AppProvider')
  return ctx
}

/** Alias used by room views. */
export const useApp = useAppState
