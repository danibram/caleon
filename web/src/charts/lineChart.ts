import type { MetricPoint, MetricsResponse } from '../api/types.ts'
import { esc } from '../lib/esc.ts'
import { isPresent } from '../lib/guards.ts'
import { iconHtml } from '../lib/iconPaths.ts'

export interface ChartSeries {
  name: string
  color: string
  points: MetricPoint[]
  hidden?: boolean
  step?: boolean
  dashed?: boolean
  noLegend?: boolean
  twins?: ChartSeries[]
  twinOf?: ChartSeries
}

export interface LineChartArgs {
  svg: SVGSVGElement
  legend: HTMLElement
  series: ChartSeries[]
  hours: number
  yUnit?: string
  lanes?: boolean
  bands?: [number, number][]
}

/** Series the user switched off in a legend stay off across redraws, per chart. */
export const hiddenSeries: Record<string, Set<string>> = {}

export function lineChart({
  svg,
  legend: legendEl,
  series,
  hours,
  yUnit = '',
  lanes = false,
  bands = [],
}: LineChartArgs): void {
  const chartKey = svg.id || svg.getAttribute('data-chart-key') || 'chart'
  const hid = (hiddenSeries[chartKey] = hiddenSeries[chartKey] || new Set())
  series.forEach((s) => {
    if (!s.twinOf) {
      s.hidden = hid.has(s.name)
      ;(s.twins || []).forEach((t) => {
        t.hidden = s.hidden
      })
    }
  })
  const W = svg.clientWidth || 800,
    H = svg.clientHeight || 260
  const narrow = W < 520
  const P = { l: lanes ? (narrow ? 118 : 190) : 38, r: 12, t: 12, b: 24 }
  const visible = series.filter((s) => !s.hidden)
  const vals = visible.flatMap((s) => s.points.map((p) => p[1])).filter(isPresent)
  if (!series.length || (!vals.length && !lanes)) {
    svg.innerHTML = ''
    svg.style.height = 'auto'
    legendEl.innerHTML =
      '<div class="chart-empty">Aún no hay datos en este periodo. El servidor guarda una muestra cada minuto.</div>'
    return
  }
  const now = Date.now() / 1000
  const first = Math.min(...visible.flatMap((s) => s.points.slice(0, 1).map((p) => p[0])), now)
  let t0 = now - hours * 3600
  const t1 = now
  if (first > t0 + (now - t0) * 0.2)
    t0 = Math.max(
      t0,
      first - Math.max(900, (now - first) * 0.08),
      now - Math.max(3 * 3600, (now - first) * 1.1),
    )
  const partial = first > now - hours * 3600 + 900
  let lo: number, hi: number
  if (lanes) {
    lo = 0
    hi = visible.length || 1
  } else if (yUnit === '%') {
    lo = Math.floor(Math.min(...vals) - 0.5)
    hi = Math.ceil(Math.max(...vals) + 0.5)
  } else {
    lo = Math.min(...vals)
    hi = Math.max(...vals)
    const pad = Math.max(0.4, (hi - lo) * 0.15)
    lo = Math.floor((lo - pad) * 2) / 2
    hi = Math.ceil((hi + pad) * 2) / 2
  }
  const x = (t: number) => P.l + ((t - t0) / Math.max(1, t1 - t0)) * (W - P.l - P.r)
  const y = (v: number) => P.t + ((hi - v) / (hi - lo)) * (H - P.t - P.b)
  let g = ''
  for (const [b0, b1] of bands)
    if (b1 > t0)
      g += `<rect class="hband" x="${x(Math.max(b0, t0))}" y="${P.t}" width="${Math.max(2, x(b1) - x(Math.max(b0, t0)))}" height="${H - P.t - P.b}"/>`
  if (!lanes) {
    const span = hi - lo,
      stepY =
        span <= 3 ? 0.5 : span <= 7 ? 1 : span <= 15 ? 2 : span <= 40 ? 5 : 10
    for (let v = Math.ceil(lo / stepY) * stepY; v <= hi + 1e-9; v += stepY)
      g += `<line class="gl" x1="${P.l}" x2="${W - P.r}" y1="${y(v)}" y2="${y(v)}"/><text x="${P.l - 6}" y="${y(v) + 4}" text-anchor="end">${String(+v.toFixed(1)).replace('.', ',')}${yUnit}</text>`
  }
  const span = t1 - t0,
    want = narrow ? (lanes ? 3 : 5) : 8
  const step =
    [1800, 3600, 7200, 10800, 21600, 43200, 86400, 172800, 604800].find(
      (st) => span / st <= want,
    ) || 604800
  const tz = -new Date().getTimezoneOffset() * 60
  for (let t = Math.ceil((t0 + tz) / step) * step - tz; t <= t1; t += step) {
    const d = new Date(t * 1000),
      xx = x(t)
    if (xx < P.l + 14 || xx > W - P.r - 14) continue
    const lab =
      step >= 86400
        ? d.toLocaleDateString('es-ES', { day: 'numeric', month: 'short' })
        : d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })
    g += `<line class="gl" x1="${xx}" x2="${xx}" y1="${H - P.b}" y2="${H - P.b + 4}"/><text x="${xx}" y="${H - 6}" text-anchor="middle">${lab}</text>`
  }
  const gap = Math.max(300, (hours * 3600) / 600 * 2.5)
  let lines = '',
    laneLabels = ''
  if (lanes && !visible.some((s) => s.points.some((p) => p[1] !== null && p[1] >= 0.5 && p[0] >= t0))) {
    svg.innerHTML = ''
    svg.style.height = '0px'
    svg.style.display = 'none'
    legendEl.innerHTML = `<div class="chart-empty">${iconHtml('power', 'mini')} Ninguna zona ha calentado en este periodo.</div>`
    return
  }
  svg.style.display = ''
  const solid = visible.filter((s) => !s.dashed)
  visible.forEach((s, k) => {
    const dy = lanes || s.dashed ? 0 : (solid.indexOf(s) - (solid.length - 1) / 2) * 1.6
    if (lanes) {
      const lh = (H - P.t - P.b) / visible.length,
        top = P.t + k * lh,
        bh = Math.min(16, lh * 0.62)
      let rects = '',
        start: number | null = null
      s.points.forEach(([ts, v], i) => {
        const next = s.points[i + 1]
        const on = v !== null && v >= 0.5
        if (on && start === null) start = ts
        if (start !== null && (!on || !next || next[0] - ts > gap)) {
          const end = on ? ts + 60 : ts
          rects += `<rect x="${x(Math.max(start, t0))}" width="${Math.max(2, x(end) - x(Math.max(start, t0)))}" y="${top + (lh - bh) / 2}" height="${bh}" fill="${s.color}" rx="${bh / 2}"/>`
          start = null
        }
      })
      // on a phone the label column is short: drop the zone letter and shorten the pump and boiler
      const short = narrow
        ? s.name.replace(/ · zona [A-Z]$/, '').replace('Bomba del circuito', 'Bomba').replace('Demanda a la caldera', 'Caldera')
        : s.name
      const maxCh = narrow ? 14 : 22,
        lab = short.length > maxCh ? short.slice(0, maxCh - 1) + '…' : short
      lines += `<rect class="lane" x="${P.l}" y="${top + (lh - bh) / 2}" width="${W - P.l - P.r}" height="${bh}" rx="${bh / 2}"/>${rects}`
      laneLabels += `<text class="lane-label" x="2" y="${top + lh / 2 + 4}"><title>${esc(s.name)}</title>${esc(lab)}</text>`
      return
    }
    let d = '',
      last: number | null = null
    for (const [ts, v] of s.points) {
      if (v === null || v === undefined || ts < t0) {
        last = null
        continue
      }
      if (last !== null && ts - last <= gap)
        d += s.step
          ? `H${x(ts).toFixed(1)}V${(y(v) + dy).toFixed(1)}`
          : `L${x(ts).toFixed(1)},${(y(v) + dy).toFixed(1)}`
      else d += `M${x(ts).toFixed(1)},${(y(v) + dy).toFixed(1)}`
      last = ts
    }
    const stroke = s.dashed ? `color-mix(in oklch, ${s.color} 55%, var(--card))` : s.color
    if (!s.dashed)
      lines += `<path d="${d}" fill="none" stroke="var(--card)" stroke-width="5" stroke-linejoin="round" stroke-linecap="round"/>`
    lines += `<path ${s.dashed ? '' : 'class="line" pathLength="1"'} d="${d}" fill="none" stroke="${stroke}" stroke-width="${s.dashed ? 1.6 : 2.2}" ${s.dashed ? 'stroke-dasharray="5 4"' : ''} stroke-linejoin="round" stroke-linecap="round"/>`
  })
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`)
  const cid = 'clip' + Math.random().toString(36).slice(2, 8)
  svg.innerHTML =
    `<defs><clipPath id="${cid}"><rect x="${P.l}" y="${P.t - 2}" width="${W - P.l - P.r}" height="${H - P.t - P.b + 4}"/></clipPath></defs>` +
    g +
    `<g clip-path="url(#${cid})">${lines}</g>` +
    laneLabels +
    `<g class="scrub" style="display:none"><line y1="${P.t}" y2="${H - P.b}"/><g class="dots"></g></g>`
  const fmtV = (v: number | null | undefined) =>
    v === null || v === undefined
      ? '—'
      : v.toFixed(1).replace('.', ',') + (yUnit === '°' ? ' °C' : yUnit === '%' ? ' %' : '')
  if (lanes) {
    // hover or touch: the time and, for each lane, whether it was on
    const wrap = svg.parentElement
    if (wrap) {
      const found = wrap.querySelector('.readout')
      let tip: HTMLElement
      if (found instanceof HTMLElement) tip = found
      else {
        tip = document.createElement('div')
        tip.className = 'readout'
        wrap.insertBefore(tip, svg)
      }
      const idle = `<span class="muted">${matchMedia('(any-pointer: coarse)').matches ? 'Toca la gráfica' : 'Pasa el ratón por la gráfica'} para ver cada momento</span>`
      tip.innerHTML = idle
      const move = (ev: PointerEvent) => {
        const rect = svg.getBoundingClientRect(),
          px = ((ev.clientX - rect.left) / rect.width) * W
        if (px < P.l || px > W - P.r) return
        const t = t0 + ((px - P.l) / (W - P.l - P.r)) * (t1 - t0)
        const rows = visible.map((s) => {
          let last: MetricPoint | null = null
          for (const p of s.points) if (p[1] !== null && p[0] <= t) last = p
          const on = last !== null && t - last[0] < gap * 4 ? last[1]! >= 0.5 : null
          return { s, on }
        })
        const sc = svg.querySelector('.scrub')
        const line = sc?.firstElementChild
        if (sc instanceof SVGGElement && line instanceof SVGLineElement) {
          sc.style.display = ''
          line.setAttribute('x1', String(px))
          line.setAttribute('x2', String(px))
        }
        tip.innerHTML =
          `<b>${new Date(t * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })}</b>` +
          rows
            .map(
              ({ s, on }) =>
                `<span><i style="background:${on ? s.color : 'var(--line)'}"></i>${esc(s.name)} <b>${on === null ? 'sin dato' : on ? 'sí' : 'no'}</b></span>`,
            )
            .join('')
      }
      svg.onpointermove = move
      svg.onpointerdown = move
      svg.onpointerleave = () => {
        const sc = svg.querySelector('.scrub')
        if (sc instanceof SVGGElement) sc.style.display = 'none'
        tip.innerHTML = idle
      }
    }
  }
  if (!lanes) {
    const wrap = svg.parentElement
    if (wrap) {
      const found = wrap.querySelector('.readout')
      let tip: HTMLElement
      if (found instanceof HTMLElement) tip = found
      else {
        tip = document.createElement('div')
        tip.className = 'readout'
        wrap.insertBefore(tip, svg)
      }
      const viewRoot = document.querySelector('#view')
      const firstReadout =
        !viewRoot?.querySelector('.readout') || wrap.querySelector('.readout') === viewRoot?.querySelector('.readout')
      const idle = firstReadout
        ? `<span class="muted">${matchMedia('(any-pointer: coarse)').matches ? 'Toca el gráfico' : 'Pasa el ratón por el gráfico'} para ver los valores</span>`
        : ''
      tip.innerHTML = idle
      const move = (ev: PointerEvent) => {
        const rect = svg.getBoundingClientRect(),
          px = ((ev.clientX - rect.left) / rect.width) * W
        if (px < P.l || px > W - P.r) return
        const t = t0 + ((px - P.l) / (W - P.l - P.r)) * (t1 - t0)
        const rows = visible
          .filter((s) => !s.twinOf)
          .map((s) => {
            let best: MetricPoint | null = null
            for (const p of s.points)
              if (p[1] !== null && (best === null || Math.abs(p[0] - t) < Math.abs(best[0] - t)))
                best = p
            return best && Math.abs(best[0] - t) < gap ? { s, v: best[1] } : null
          })
          .filter(isPresent)
        const sc = svg.querySelector('.scrub')
        const line = sc?.firstElementChild
        const dots = sc?.querySelector('.dots')
        if (!(sc instanceof SVGGElement) || !(line instanceof SVGLineElement) || !dots) return
        sc.style.display = ''
        line.setAttribute('x1', String(px))
        line.setAttribute('x2', String(px))
        dots.innerHTML = rows
          .flatMap(({ s, v }) =>
            v !== null && v >= lo && v <= hi
              ? `<circle class="dotc" cx="${px}" cy="${y(v)}" r="4.5" fill="${s.dashed ? `color-mix(in oklch, ${s.color} 55%, var(--card))` : s.color}"/>`
              : [],
          )
          .join('')
        tip.innerHTML =
          `<b>${new Date(t * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })}</b>` +
          rows
            .map(
              ({ s, v }) =>
                `<span><i style="background:${s.dashed ? `color-mix(in oklch, ${s.color} 55%, var(--card))` : s.color}"></i>${esc(s.name)} <b>${fmtV(v)}</b></span>`,
            )
            .join('')
      }
      svg.onpointermove = move
      svg.onpointerdown = move
      svg.onpointerleave = () => {
        const sc = svg.querySelector('.scrub')
        if (sc instanceof SVGGElement) sc.style.display = 'none'
        tip.innerHTML = idle
      }
    }
  }
  const sinceTxt = new Date(first * 1000).toLocaleTimeString('es-ES', {
    hour: '2-digit',
    minute: '2-digit',
  })
  const note = partial
    ? `<span class="chart-note">Solo hay datos desde las ${sinceTxt}${first < now - 86400 ? ' del ' + new Date(first * 1000).toLocaleDateString('es-ES', { day: 'numeric', month: 'short' }) : ''}</span>`
    : ''
  const shown = series.filter((s) => !s.noLegend)
  legendEl.innerHTML =
    (shown.length === 1
      ? ''
      : shown
          .map(
            (s) =>
              `<button class="lg ${s.hidden ? 'off' : ''}" data-i="${series.indexOf(s)}" aria-pressed="${!s.hidden}"><i style="background:${s.color}"></i>${esc(s.name)}</button>`,
          )
          .join('')) + note
  legendEl.onclick = (ev) => {
    const b = ev.target instanceof Element ? ev.target.closest('.lg') : null
    const s = b && series[Number(b.getAttribute('data-i'))]
    if (!s) return
    s.hidden = !s.hidden
    if (s.hidden) hid.add(s.name)
    else hid.delete(s.name)
    ;(s.twins || []).forEach((t) => {
      t.hidden = s.hidden
    })
    lineChart({ svg, legend: legendEl, series, hours, yUnit, lanes, bands })
  }
}

/** Time ranges in which any of `keys` was on, from /api/metrics. */
export function onIntervals(metrics: MetricsResponse, keys: string[]): [number, number][] {
  const pts = keys
    .flatMap((k) => (metrics[k] || []).map((p): [number, boolean] => [p[0], p[1] !== null && p[1] >= 0.5]))
    .sort((a, b) => a[0] - b[0])
  const out: [number, number][] = []
  let start: number | null = null
  for (const [ts, on] of pts) {
    if (on && start === null) start = ts
    if (!on && start !== null) {
      out.push([start, ts])
      start = null
    }
  }
  if (start !== null) out.push([start, Date.now() / 1000])
  return out
}

/** 5-sample median, then a 0,15° dead band, so the sensor's 0,1° flicker never draws as steps. */
export function deblip(pts: MetricPoint[], band = 0.15): MetricPoint[] {
  const med = pts.map((p, i): MetricPoint => {
    if (p[1] === null) return p
    const w = pts
      .slice(Math.max(0, i - 2), i + 3)
      .map((q) => q[1])
      .filter(isPresent)
    w.sort((a, b) => a - b)
    return [p[0], w[Math.floor(w.length / 2)]]
  })
  let last: number | null = null
  return med.map((p) => {
    if (p[1] === null) {
      last = null
      return p
    }
    if (last === null || Math.abs(p[1] - last) >= band) last = p[1]
    return [p[0], last]
  })
}
