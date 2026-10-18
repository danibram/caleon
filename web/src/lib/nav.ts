/** Where "back" goes from a page that is not a tab, with the parent's name like iOS does. */
export function backTarget(pathname: string, roomName: (id: number) => string | null): { to: string; label: string } | null {
  let m = /^\/room\/(\d+)$/.exec(pathname)
  if (m) return { to: '/', label: 'Inicio' }
  m = /^\/timer\/(\d+)$/.exec(pathname)
  if (m) return { to: `/room/${m[1]}`, label: roomName(Number(m[1])) ?? 'Habitación' }
  if (pathname === '/backups' || pathname === '/info') return { to: '/settings', label: 'Ajustes' }
  return null
}

