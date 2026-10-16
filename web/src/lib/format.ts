export const es = (text: string | null | undefined) =>
  String(text ?? '')
    .replace(/(\d)\.(\d)/g, '$1,$2')
    .replace(/(\d)(°C|%)/g, '$1 $2')
    .replace(/% RH\b/g, '% HR')

export const shortErr = (m: string | null | undefined) =>
  String(m || '')
    .replace(/^(No se pudo \w+|No se aplicó)\s*[:·]\s*/i, '')
    .replace(/^Modo solo lectura:.*$/i, 'modo solo lectura')
    .replace(/^[a-z_0-9]+\.json:\s*/i, '')
    .replace(/:? ?sin respuesta del equipo/i, 'el termostato no respondió')
    .replace(/\s+/g, ' ')
    .slice(0, 110)

export function fmtC(v: number | null | undefined): string {
  return v === null || v === undefined ? '--' : v.toFixed(1).replace('.', ',')
}

export function num(s: string | undefined | null): number | null {
  const m = /-?\d+(?:[.,]\d+)?/.exec(s || '')
  return m ? parseFloat(m[0].replace(',', '.')) : null
}

export function fmtFirmware(f: string | null | undefined): string {
  const m = /^(\d{4}-\d{2}-\d{2})\s+(\d+)$/.exec(f || '')
  return m
    ? `${new Date(m[1] + 'T12:00').toLocaleDateString('es-ES', {
        day: 'numeric',
        month: 'short',
        year: 'numeric',
      })} (${m[2]})`
    : f || '—'
}
