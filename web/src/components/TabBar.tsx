import { Link, useLocation } from 'react-router'
import { Icon } from './Icon.tsx'
import { NAV, TABS } from '../lib/icons.ts'

const TAB_ITEMS = NAV.filter(([h]) => TABS.includes(h))

function isActive({ path, current, inTabbar }: { path: string; current: string; inTabbar: boolean }) {
  const inMore =
    inTabbar &&
    ['/backups', '/info'].some((x) => current.startsWith(x)) &&
    path === '/settings'
  if (inMore) return true
  if (path === '/')
    return current === '/' || current.startsWith('/room') || current.startsWith('/timer')
  return current.startsWith(path)
}

export function TabBar() {
  const { pathname } = useLocation()

  return (
    <nav className="tabbar" aria-label="Secciones">
      {TAB_ITEMS.map(([h, l, i]) => (
        <Link
          key={h}
          to={h}
          viewTransition
          className={isActive({ path: h, current: pathname, inTabbar: true }) ? 'active' : ''}
        >
          <Icon name={i} />
          <span>{l}</span>
        </Link>
      ))}
    </nav>
  )
}

export function TopNav() {
  const { pathname } = useLocation()

  return (
    <nav id="nav">
      {NAV.map(([h, l, i]) => (
        <Link key={h} to={h} viewTransition className={isActive({ path: h, current: pathname, inTabbar: false }) ? 'active' : ''}>
          <Icon name={i} />
          <span className="nav-label">{l}</span>
        </Link>
      ))}
    </nav>
  )
}
