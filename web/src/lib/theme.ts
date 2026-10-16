export type ThemeChoice = 'system' | 'light' | 'dark'

export function applyTheme(t?: ThemeChoice): ThemeChoice {
  try {
    if (t) localStorage.setItem('caleon.theme', t)
  } catch {
    /* private mode */
  }
  let v: ThemeChoice = 'system'
  try {
    v = (localStorage.getItem('caleon.theme') as ThemeChoice) || 'system'
  } catch {
    /* ignore */
  }
  if (v === 'system') document.documentElement.removeAttribute('data-theme')
  else document.documentElement.dataset.theme = v
  return v
}
