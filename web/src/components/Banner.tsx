import type { ReactNode } from 'react'
import { useAppState } from '../context/AppContext'
import { useNow } from '../hooks/useNow'
import { Icon } from './Icon'

function outageSince(S: NonNullable<ReturnType<typeof useAppState>['S']>) {
  return S.updated
    ? new Date(S.updated * 1000).toLocaleTimeString('es-ES', {
        hour: '2-digit',
        minute: '2-digit',
      })
    : null
}

export function Banner() {
  const { S } = useAppState()
  const now = useNow()
  if (!S) return null

  const parts: ReactNode[] = []

  if (S.read_only) {
    parts.push(
      <div className="ro-banner" key="ro">
        <Icon name="lock" className="mini" /> Modo solo lectura. Puedes mirarlo todo, pero este
        servidor no cambia el termostato.
      </div>,
    )
  }

  const age = S.updated ? now / 1000 - S.updated : 1e9
  if (S.error || age > 180) {
    const since = outageSince(S)
    parts.push(
      <div className="banner" key="outage">
        <Icon name="warn" className="mini" />{' '}
        {since
          ? `No hay respuesta del termostato desde las ${since}.`
          : 'No hay respuesta del termostato.'}{' '}
        Lo que ves son los últimos datos; sigo intentándolo.
      </div>,
    )
  } else if (S.crawling) {
    parts.push(
      <div className="banner" key="crawl">
        <span className="spin" /> Leyendo la configuración completa del equipo (copia de
        seguridad)…
      </div>,
    )
  }

  if (!parts.length) return null
  return <>{parts}</>
}
