import { Link, useLocation } from 'react-router'
import { useAppState } from '../context/AppContext.tsx'
import { roomById, roomTitle } from '../lib/rooms.ts'
import { backTarget } from '../lib/nav.ts'
import { Icon } from './Icon.tsx'

/** The big back control in the top bar, shown on every page that has a parent. */
export function BackButton() {
  const { S } = useAppState()
  const { pathname } = useLocation()
  const target = backTarget(pathname, (id) => {
    const r = roomById(S, id)
    return r ? roomTitle(r) : null
  })
  if (!target) return null
  return (
    <Link to={target.to} className="hdr-back" viewTransition aria-label={`Volver a ${target.label}`}>
      <Icon name="back" />
      <span>{target.label}</span>
    </Link>
  )
}
