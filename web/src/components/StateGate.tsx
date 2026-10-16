import type { ReactNode } from 'react'
import { useApp } from '../context/AppContext.tsx'

/** Holds a view back until the first snapshot has arrived. */
export function StateGate({ children }: { children: ReactNode }) {
  const { S } = useApp()
  if (!S) {
    return (
      <div className="empty">
        <span className="spin" /> Conectando con el termostato…
      </div>
    )
  }
  return children
}
