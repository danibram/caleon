import { useAppState } from '../context/AppContext.tsx'
import { useNow } from '../hooks/useNow.ts'

function ago(secs: number) {
  if (secs < 90) return 'ahora mismo'
  if (secs < 3600) return `hace ${Math.round(secs / 60)} min`
  return `hace ${Math.round(secs / 3600)} h`
}

export function MetaHeader() {
  const { S } = useAppState()
  const now = useNow()

  if (!S) {
    return (
      <div className="meta" title="">
        <span className="live" />
        <span>Conectando…</span>
      </div>
    )
  }

  const age = S.updated ? now / 1000 - S.updated : 1e9
  const cls = S.error || age > 300 ? 'err' : age > 120 ? 'stale' : ''
  const t = S.updated
    ? new Date(S.updated * 1000).toLocaleTimeString('es-ES', {
        hour: '2-digit',
        minute: '2-digit',
      })
    : '—'

  let text: string
  if (S.crawling) text = 'Leyendo la configuración…'
  else if (S.error) text = 'El termostato no responde'
  else if (age > 120) text = `Datos de ${ago(age)}`
  else text = `Actualizado ${t}`

  const title = `${S.device} · firmware ${S.firmware || '?'} · última lectura ${t}`

  return (
    <div className="meta" title={title}>
      <span className={`live ${cls}`} />
      <span>{text}</span>
    </div>
  )
}
