/** Router state the menu attaches when it only rewrites the URL of the screen already open. */
export const MENU_SYNC_STATE = { menuSync: true }

export const hasMenuSync = (state: unknown): boolean =>
  typeof state === 'object' && state !== null && 'menuSync' in state && state.menuSync === true
