import type { Room, Snapshot } from '../context/AppContext.tsx'
import { MODE_LABEL } from './constants.ts'
import { fmtC, num } from './format.ts'
import { isHeating, modeOf, modeTemp } from './rooms.ts'

export function UnknownModesNote({ room: r, S }: { room: Room; S: Snapshot | null }) {
  const missing = (['eco', 'turbo'] as const)
    .filter((m) => modeTemp(r, m, S) === null)
    .map((m) => MODE_LABEL[m])
  if (!missing.length) return null
  return (
    <p className="forecast small-note">
      La temperatura de {missing.join(' y ')} aparecerá cuando la habitación pase un rato en ese modo. El
      termostato no la comunica de otra forma.
    </p>
  )
}

export function Forecast({ room, S }: { room: Room; S: Snapshot | null }) {
  const t = num(room.temperature)
  const mode = modeOf(room)
  const sp = num(room.tset)
  if (t === null || room.vacation) return null
  if (mode === 'off') {
    const nt = modeTemp(room, 'regular', S)
    return (
      <>
        {nt !== null ? (
          <p className="forecast">
            En Normal (<b>{fmtC(nt)}°</b>){' '}
            {t >= nt
              ? `no calentaría ahora, porque la habitación ya está a ${fmtC(t)}°.`
              : `empezaría a calentar, porque ahora está a ${fmtC(t)}°.`}
          </p>
        ) : null}
        <p className="forecast small-note">
          Apagada sigue protegida contra heladas. Un modo elegido a mano se mantiene hasta el siguiente
          cambio del horario.
        </p>
      </>
    )
  }
  if (sp === null) return null
  if (isHeating(room)) {
    return (
      <p className="forecast">
        Calentando hasta <b>{fmtC(sp)}°</b>.
      </p>
    )
  }
  if (t >= sp) {
    return (
      <p className="forecast">
        No calienta porque ya está a {fmtC(t)}°, por encima de su objetivo de <b>{fmtC(sp)}°</b>.
      </p>
    )
  }
  return (
    <p className="forecast">
      Por debajo de su objetivo (<b>{fmtC(sp)}°</b>), pero ahora mismo no está calentando.
    </p>
  )
}
