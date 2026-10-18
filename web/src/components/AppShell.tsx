import { useEffect, useLayoutEffect, useRef } from 'react'
import { Link, Outlet, useLocation, useNavigationType } from 'react-router'
import { useAppState } from '../context/AppContext.tsx'
import { hasMenuSync } from '../lib/menuSync.ts'
import { applyTheme } from '../lib/theme.ts'
import { BackButton } from './BackButton.tsx'
import { MetaHeader } from './MetaHeader.tsx'
import { TabBar, TopNav } from './TabBar.tsx'
import { ToastLayer } from './Toast.tsx'

const depthOf = (p: string) =>
  p.startsWith('/timer') ? 2 : p.startsWith('/room') || p === '/info' || p === '/backups' ? 1 : 0

export function AppShell() {
  const { staleData, bumpNav } = useAppState()
  const location = useLocation()
  const navType = useNavigationType()
  const lastPath = useRef<string | null>(null)
  // window.scrollY is already clamped to the new page's height when the layout effect runs
  const lastScrollY = useRef(0)
  const scrollMem = useRef<Record<string, number>>({})
  const headerRef = useRef<HTMLElement>(null)

  useEffect(() => {
    applyTheme()
    if (!document.startViewTransition) document.documentElement.classList.add('no-vt')
  }, [])

  // a layout effect, because it must run before the new view's effects start their requests
  useLayoutEffect(() => {
    const path = location.pathname
    if (lastPath.current === path) return
    const menuSync = navType === 'REPLACE' && hasMenuSync(location.state)
    if (lastPath.current !== null) scrollMem.current[lastPath.current] = lastScrollY.current
    const d0 = lastPath.current === null ? 0 : depthOf(lastPath.current)
    const d1 = depthOf(path)
    const nav =
      lastPath.current === null ? 'same' : d1 > d0 ? 'forward' : d1 < d0 ? 'back' : 'same'
    lastPath.current = path
    if (menuSync) return
    bumpNav()
    document.documentElement.dataset.nav = nav
    window.scrollTo(0, nav === 'back' ? scrollMem.current[path] || 0 : 0)
  }, [location.pathname, location.state, navType, bumpNav])

  useEffect(() => {
    const onScroll = () => {
      lastScrollY.current = window.scrollY
    }
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  useEffect(() => {
    const setHdr = () => {
      const h = headerRef.current
      if (h) document.documentElement.style.setProperty('--hdr', h.offsetHeight + 'px')
    }
    setHdr()
    window.addEventListener('resize', setHdr)
    return () => window.removeEventListener('resize', setHdr)
  }, [])

  return (
    <>
      <header ref={headerRef}>
        <div className="bar">
          <BackButton />
          <Link to="/" className="brand">
            <h1>
              <svg className="mark" viewBox="0 0 64 64" aria-hidden="true">
                <rect width="64" height="64" rx="16" />
                <path d="M32 12c-3.9 0-7 3.1-7 7v18a11 11 0 1 0 14 0V19c0-3.9-3.1-7-7-7zM32 44V24" />
                <circle cx="32" cy="45" r="5.5" />
              </svg>
              <span>
                Open<span className="dim">Caleon</span>
              </span>
            </h1>
          </Link>
          <MetaHeader />
        </div>
        <TopNav />
      </header>
      <main id="view" className={staleData ? 'stale-data' : ''}>
        <Outlet />
      </main>
      <TabBar />
      <ToastLayer />
    </>
  )
}
