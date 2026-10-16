import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { AskDialog, showAskDialog, type AskUiState } from '../components/AskDialog.tsx'
import { shortErr } from '../lib/format.ts'

import type { Snapshot } from '../api/types.ts'

export type { Room, RoomOutput, Snapshot, StatusMetric } from '../api/types.ts'

type ApiOpts = {
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

type AppContextValue = {
  S: Snapshot | null
  setS: (value: Snapshot | null | ((prev: Snapshot | null) => Snapshot | null)) => void
  reloadState: () => Promise<unknown>
  api: <T = unknown>(path: string, opts?: ApiOpts) => Promise<T>
  toast: (msg: string, ms?: number) => void
  ask: (opts: AskOptions) => Promise<string | boolean | null>
  rereadSoon: () => void
  navGen: { current: number }
  bumpNav: () => void
  roomBusy: Set<number>
  setRoomBusy: (id: number, busy: boolean) => void
  staleData: boolean
  viewRefreshRef: { current: (() => void) | null }
  refreshView: () => void
}

const STALE = new Promise<never>(() => {})

const AppContext = createContext<AppContextValue | null>(null)

async function fetchState(): Promise<Snapshot> {
  const r = await fetch('/api/state')
  const data = (await r.json().catch(() => ({}))) as Snapshot & { error?: string }
  if (!r.ok) throw new Error(data.error || r.statusText)
  return data
}

export function AppProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const navGen = useRef(0)
  const rereadTimers = useRef<ReturnType<typeof setTimeout>[]>([])
  const viewRefreshRef = useRef<(() => void) | null>(null)
  const dlgRef = useRef<HTMLDialogElement>(null)
  const askResolve = useRef<((v: string | boolean | null) => void) | null>(null)
  const [roomBusy, setRoomBusyState] = useState(() => new Set<number>())
  const [askUi, setAskUi] = useState<AskUiState | null>(null)

  const { data: S = null, refetch } = useQuery({
    queryKey: ['state'],
    queryFn: fetchState,
    refetchInterval: 15000,
    retry: 2,
  })

  useEffect(() => {
    viewRefreshRef.current?.()
  }, [S])

  const reloadState = useCallback(() => refetch(), [refetch])

  const setS = useCallback(
    (value: Snapshot | null | ((prev: Snapshot | null) => Snapshot | null)) => {
      queryClient.setQueryData(['state'], value)
    },
    [queryClient],
  )

  const bumpNav = useCallback(() => {
    navGen.current += 1
    const dlg = dlgRef.current
    if (dlg?.open) {
      dlg.returnValue = ''
      dlg.close()
    }
  }, [])

  const toast = useCallback((msg: string, ms = 3500) => {
    let text = msg
    if (/^(No se pudo|No se aplicó|Modo solo lectura)/i.test(text)) {
      text = 'No se aplicó · ' + shortErr(text)
    }
    const t = document.createElement('div')
    t.className = 'toast'
    t.textContent = text
    document.body.appendChild(t)
    setTimeout(() => t.remove(), ms)
  }, [])

  const api = useCallback(async <T,>(path: string, opts: ApiOpts = {}): Promise<T> => {
    const seq = navGen.current
    const init: RequestInit = {
      method: opts.method || (opts.body !== undefined ? 'POST' : 'GET'),
      headers: {},
    }
    if (opts.body !== undefined) {
      ;(init.headers as Record<string, string>)['Content-Type'] = 'application/json'
      init.body = JSON.stringify(opts.body)
    }
    const r = await fetch(path, init)
    const data = await r.json().catch(() => ({}))
    if (!opts.any && seq !== navGen.current) return STALE as Promise<T>
    if (!r.ok) throw new Error((data as { error?: string }).error || r.statusText)
    return data as T
  }, [])

  const rereadSoon = useCallback(() => {
    rereadTimers.current.forEach(clearTimeout)
    rereadTimers.current = [7, 18, 33].map((s) =>
      setTimeout(() => {
        void refetch()
      }, s * 1000),
    )
  }, [refetch])

  const setRoomBusy = useCallback((id: number, busy: boolean) => {
    setRoomBusyState((prev) => {
      const next = new Set(prev)
      if (busy) next.add(id)
      else next.delete(id)
      return next
    })
  }, [])

  const refreshView = useCallback(() => {
    viewRefreshRef.current?.()
  }, [])

  const ask = useCallback((opts: AskOptions) => {
    return new Promise<string | boolean | null>((resolve) => {
      askResolve.current = resolve
      setAskUi({
        ...opts,
        input: opts.input !== false,
        inputValue: opts.value ?? '',
      })
      requestAnimationFrame(() => showAskDialog(dlgRef.current))
    })
  }, [])

  const resolveAsk = useCallback((value: string | boolean | null) => {
    askResolve.current?.(value)
    askResolve.current = null
  }, [])

  const age = S?.updated ? Date.now() / 1000 - S.updated : 1e9
  const staleData = !!(S?.error || age > 180)

  const value = useMemo(
    () => ({
      S,
      setS,
      reloadState,
      api,
      toast,
      ask,
      rereadSoon,
      navGen,
      bumpNav,
      roomBusy,
      setRoomBusy,
      staleData,
      viewRefreshRef,
      refreshView,
    }),
    [
      S,
      setS,
      reloadState,
      api,
      toast,
      ask,
      rereadSoon,
      bumpNav,
      roomBusy,
      setRoomBusy,
      staleData,
      refreshView,
    ],
  )

  return (
    <AppContext.Provider value={value}>
      {children}
      <AskDialog ref={dlgRef} askUi={askUi} setAskUi={setAskUi} onResolve={resolveAsk} />
    </AppContext.Provider>
  )
}

export function useAppState() {
  const ctx = useContext(AppContext)
  if (!ctx) throw new Error('useAppState must be used within AppProvider')
  return ctx
}

/** Alias used by room views. */
export const useApp = useAppState
