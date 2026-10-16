import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useCallback, useMemo, useRef, useState, type ReactNode } from 'react'
import { api as request, type ApiOptions } from '../api/client.ts'
import { AskDialog, type AskUiState } from '../components/AskDialog.tsx'
import { useNow } from '../hooks/useNow.ts'
import { shortErr } from '../lib/format.ts'
import { AppContext, type AskOptions } from './AppContext.tsx'

import type { Snapshot } from '../api/types.ts'

const STALE = new Promise<never>(() => {})

const fetchState = () => request<Snapshot>('/api/state')

export function AppProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const navGenRef = useRef(0)
  const rereadTimers = useRef<ReturnType<typeof setTimeout>[]>([])
  const dlgRef = useRef<HTMLDialogElement>(null)
  const askResolve = useRef<((v: string | boolean | null) => void) | null>(null)
  const [roomBusy, setRoomBusyState] = useState(() => new Set<number>())
  const [askUi, setAskUi] = useState<AskUiState | null>(null)
  const now = useNow()

  const { data: S = null, refetch } = useQuery({
    queryKey: ['state'],
    queryFn: fetchState,
    refetchInterval: 15000,
    retry: 2,
  })

  const reloadState = useCallback(() => refetch(), [refetch])

  const setS = useCallback(
    (value: Snapshot | null | ((prev: Snapshot | null) => Snapshot | null)) => {
      queryClient.setQueryData(['state'], value)
    },
    [queryClient],
  )

  const navGen = useCallback(() => navGenRef.current, [])

  const bumpNav = useCallback(() => {
    navGenRef.current += 1
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

  // a reply for a view the user already left neither resolves nor rejects
  const api = useCallback(async <T,>(path: string, opts: ApiOptions = {}): Promise<T> => {
    const seq = navGenRef.current
    const [res] = await Promise.allSettled([request<T>(path, opts)])
    if (!opts.any && seq !== navGenRef.current) return STALE
    if (res.status === 'rejected') throw res.reason
    return res.value
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

  const ask = useCallback((opts: AskOptions) => {
    return new Promise<string | boolean | null>((resolve) => {
      askResolve.current = resolve
      setAskUi({ ...opts, inputValue: opts.input === false ? '' : (opts.value ?? '') })
      // two frames: the dialog content has to be committed before showModal() focuses it
      requestAnimationFrame(() => requestAnimationFrame(() => dlgRef.current?.showModal()))
    })
  }, [])

  const resolveAsk = useCallback((value: string | boolean | null) => {
    askResolve.current?.(value)
    askResolve.current = null
  }, [])

  const age = S?.updated ? now / 1000 - S.updated : 1e9
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
    }),
    [S, setS, reloadState, api, toast, ask, rereadSoon, navGen, bumpNav, roomBusy, setRoomBusy, staleData],
  )

  return (
    <AppContext.Provider value={value}>
      {children}
      <AskDialog ref={dlgRef} askUi={askUi} setAskUi={setAskUi} onResolve={resolveAsk} />
    </AppContext.Provider>
  )
}
