export const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v)

export const isPresent = <T>(v: T | null | undefined): v is T => v != null
