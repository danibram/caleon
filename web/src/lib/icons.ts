import type { IconName } from '../components/Icon.tsx'

export type NavItem = [path: string, label: string, icon: IconName]

export const NAV: NavItem[] = [
  ['/', 'Inicio', 'home'],
  ['/status', 'Instalación', 'pulse'],
  ['/charts', 'Gráficas', 'chart'],
  ['/menu', 'Experto', 'sliders'],
  ['/backups', 'Copias', 'archive'],
  ['/info', 'Avisos', 'bell'],
  ['/settings', 'Ajustes', 'gear'],
]

export const TABS = ['/', '/status', '/charts', '/menu', '/settings']
