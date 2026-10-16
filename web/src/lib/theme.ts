export const THEMES = ['system', 'light', 'dark'] as const

export type ThemeChoice = (typeof THEMES)[number]

const isThemeChoice = (v: unknown): v is ThemeChoice =>
  typeof v === 'string' && THEMES.some((t) => t === v)

export function applyTheme(t?: ThemeChoice): ThemeChoice {
  try {
    if (t) localStorage.setItem('caleon.theme', t)
  } catch {
    /* private mode */
  }
  let v: ThemeChoice = 'system'
  try {
    const stored: unknown = localStorage.getItem('caleon.theme')
    if (isThemeChoice(stored)) v = stored
  } catch {
    /* ignore */
  }
  if (v === 'system') document.documentElement.removeAttribute('data-theme')
  else document.documentElement.dataset.theme = v
  return v
}
