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
  gapSeconds?: number
  /** name for the readout, when the legend name is longer than it needs to be */
  short?: string
  /** the live value, read out with no cursor instead of the last stored sample */
  current?: number | null
  /** a word for the readout when the series has no target, e.g. a room switched off */
  note?: string
  /** on/off states (one series per valve) that make the readout say the room is heating */
  heat?: MetricPoint[][]
  /** in a lanes readout, only lanes with this name are read out, as "name: encendida" */
  readoutName?: string
}

export interface LineChartArgs {
  svg: SVGSVGElement
  legend: HTMLElement
  series: ChartSeries[]
  hours: number
  yUnit?: string
  lanes?: boolean
  bands?: [number, number][]
  timeRange?: [number, number]
  emptyMessage?: string
  /** charts with the same key share the hover cursor; they need the same timeRange */
  sync?: string
  /** where the values under the cursor go; by default a line above the chart */
  readout?: HTMLElement | null
  /** names the chart in its readout, for a readout shared by stacked panels */
  label?: string
  showTime?: boolean
  /** false for a panel stacked above another that carries the time axis */
  axis?: boolean
  /** lanes without names: thin strips coloured like their series */
  laneNames?: boolean
  /** a tooltip that follows the cursor over the chart; hidden with no cursor */
  floating?: HTMLElement | null
}

type ShowAt = (t: number | null) => void

/** Charts that share a `sync` key draw one cursor at the same time on all of them. */
const groups = new Map<string, Map<SVGSVGElement, ShowAt>>()
const cursors = new Map<string, number>()

function join(group: string, svg: SVGSVGElement, show: ShowAt) {
  let members = groups.get(group)
  if (!members) groups.set(group, (members = new Map()))
  members.set(svg, show)
}

function broadcast(group: string, t: number | null) {
  if (t === null) cursors.delete(group)
  else cursors.set(group, t)
  const members = groups.get(group)
  if (!members) return
  for (const [svg, show] of members) {
    if (svg.isConnected) show(t)
    else members.delete(svg)
  }
}

/** Drops a chart from its sync group, for unmount or a redraw without a plot. */
export function unsync(svg: SVGSVGElement): void {
  for (const members of groups.values()) members.delete(svg)
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
  timeRange,
  emptyMessage,
  sync,
  readout,
  label,
  showTime = true,
  axis = true,
  laneNames = true,
  floating,
}: LineChartArgs): void {
  unsync(svg)
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
  const P = { l: 48, r: 12, t: lanes ? 4 : 12, b: axis ? 24 : 6 }
  const visible = series.filter((s) => !s.hidden)
  const vals = visible.flatMap((s) => s.points.map((p) => p[1])).filter(isPresent)
  // the legend buttons toggle series; they stay even with every series off, so they can come back
  const renderLegend = (note: string) => {
    const shown = series.filter((s) => !s.noLegend)
    legendEl.innerHTML =
      (shown.length === 1
        ? ''
        : shown
            .map(
              (s) =>
                `<button class="lg ${s.hidden ? 'off' : ''} ${s.dashed ? 'dash' : ''}" data-i="${series.indexOf(s)}" aria-pressed="${!s.hidden}"><i style="--c:${s.color}"></i>${esc(s.name)}</button>`,
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
      lineChart({ svg, legend: legendEl, series, hours, yUnit, lanes, bands, timeRange, emptyMessage, sync, readout, label, showTime, axis, laneNames, floating })
    }
  }
  if (series.length && !visible.length && !lanes) {
    svg.innerHTML = ''
    renderLegend('<span class="chart-note">Todas ocultas. Pulsa una para mostrarla.</span>')
    return
  }
  if (!series.length || (!vals.length && !lanes)) {
    svg.innerHTML = ''
    svg.style.height = 'auto'
    legendEl.innerHTML =
      `<div class="chart-empty">${esc(emptyMessage ?? 'Aún no hay datos en este periodo. El servidor guarda una muestra cada minuto.')}</div>`
    return
  }
  const now = timeRange?.[1] ?? Date.now() / 1000
  const first = Math.min(...visible.flatMap((s) => s.points.slice(0, 1).map((p) => p[0])), now)
  let t0 = timeRange?.[0] ?? now - hours * 3600
  const t1 = now
  if (!timeRange && first > t0 + (now - t0) * 0.2)
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
      // at least ~22px between gridlines, so a short panel keeps a readable scale
      rows = Math.max(3, Math.floor((H - P.t - P.b) / 18)),
      stepY = [0.5, 1, 2, 2.5, 5, 10, 20].find((st) => span / st <= rows) ?? 20
    for (let v = Math.ceil(lo / stepY) * stepY; v <= hi + 1e-9; v += stepY)
      g += `<line class="gl" x1="${P.l}" x2="${W - P.r}" y1="${y(v)}" y2="${y(v)}"/><text x="${P.l - 6}" y="${y(v) + 4}" text-anchor="end">${String(+v.toFixed(1)).replace('.', ',')}${yUnit}</text>`
  }
  const span = t1 - t0,
    want = narrow ? 5 : 8
  const step =
    [1800, 3600, 7200, 10800, 21600, 43200, 86400, 172800, 604800].find(
      (st) => span / st <= want,
    ) || 604800
  const tz = -new Date().getTimezoneOffset() * 60
  for (let t = Math.ceil((t0 + tz) / step) * step - tz; t <= t1; t += step) {
    const d = new Date(t * 1000),
      xx = x(t)
    if (xx < P.l + 14 || xx > W - P.r - 14) continue
    g += `<line class="gl vgl" x1="${xx}" x2="${xx}" y1="${P.t}" y2="${H - P.b}"/>`
    if (!axis) continue
    const lab =
      step >= 86400
        ? d.toLocaleDateString('es-ES', { day: 'numeric', month: 'short' })
        : d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })
    g += `<line class="gl" x1="${xx}" x2="${xx}" y1="${H - P.b}" y2="${H - P.b + 4}"/><text x="${xx}" y="${H - 6}" text-anchor="middle">${lab}</text>`
  }
  const gap = Math.max(300, (hours * 3600) / 600 * 2.5)
  let lines = '',
    laneLabels = ''
  if (
    lanes &&
    laneNames &&
    !visible.some((s) => s.points.some((p) => p[1] !== null && p[1] >= 0.5 && p[0] >= t0))
  ) {
    svg.innerHTML = ''
    svg.style.height = '0px'
    svg.style.display = 'none'
    legendEl.innerHTML = `<div class="chart-empty">${iconHtml('power', 'mini')} Ninguna zona ha calentado en este periodo.</div>`
    return
  }
  svg.style.display = ''
  const solid = visible.filter((s) => !s.dashed)
  visible.forEach((s, k) => {
    const seriesGap = s.gapSeconds ?? gap
    const dy = lanes || s.dashed ? 0 : (solid.indexOf(s) - (solid.length - 1) / 2) * 1.6
    if (lanes) {
      // the name sits above its bar, so the plot keeps the same left edge as the other charts
      const lh = (H - P.t - P.b) / visible.length,
        top = P.t + k * lh,
        bh = laneNames ? Math.max(4, Math.min(10, lh - 18)) : Math.max(3, lh - 3),
        barY = laneNames ? top + lh - bh - 4 : top + (lh - bh) / 2
      let rects = '',
        start: number | null = null
      s.points.forEach(([ts, v], i) => {
        const next = s.points[i + 1]
        const on = v !== null && v >= 0.5
        if (on && start === null) start = ts
        if (start !== null && (!on || !next || next[0] - ts > gap)) {
          const end = on ? ts + 60 : ts
          if (end <= t0) {
            start = null
            return
          }
          rects += `<rect x="${x(Math.max(start, t0))}" width="${Math.max(2, x(end) - x(Math.max(start, t0)))}" y="${barY}" height="${bh}" fill="${s.color}" rx="2"/>`
          start = null
        }
      })
      lines += `<rect class="lane" x="${P.l}" y="${barY}" width="${W - P.l - P.r}" height="${bh}" rx="2"/>${rects}`
      if (laneNames) laneLabels += `<text class="lane-label" x="${P.l}" y="${barY - 4}">${esc(s.name)}</text>`
      else if (s.readoutName)
        laneLabels += `<text class="lane-tag" x="${P.l - 6}" y="${barY + bh / 2 + 3.5}" text-anchor="end">${esc(s.readoutName)}</text>`
      return
    }
    let d = '',
      last: number | null = null,
      hd = '',
      hlast: number | null = null
    const heating = s.heat ? heatTimeline(s.heat, gap * 4) : null
    for (const [ts, v] of s.points) {
      if (v === null || v === undefined) {
        last = null
        hlast = null
        continue
      }
      // the same line again, thick, while one of the room's valves is open
      if (heating) {
        const yy = (y(v) + dy).toFixed(1),
          xx = x(ts).toFixed(1)
        if (heating(ts)) {
          if (hlast !== null && ts - hlast <= seriesGap) hd += s.step ? `H${xx}V${yy}` : `L${xx},${yy}`
          else hd += `M${xx},${yy}`
          hlast = ts
        } else {
          if (hlast !== null) hd += s.step ? `H${xx}` : `L${xx},${yy}`
          hlast = null
        }
      }
      if (last !== null && ts - last <= seriesGap)
        d += s.step
          ? `H${x(ts).toFixed(1)}V${(y(v) + dy).toFixed(1)}`
          : `L${x(ts).toFixed(1)},${(y(v) + dy).toFixed(1)}`
      else d += `M${x(ts).toFixed(1)},${(y(v) + dy).toFixed(1)}`
      last = ts
    }
    if (!s.dashed)
      lines += `<path d="${d}" fill="none" stroke="var(--card)" stroke-width="5" stroke-linejoin="round" stroke-linecap="round"/>`
    lines += `<path ${s.dashed ? '' : 'class="line" pathLength="1"'} d="${d}" fill="none" stroke="${s.color}" stroke-width="${s.dashed ? 1.6 : 2.2}" ${s.dashed ? 'stroke-dasharray="5 4" class="target"' : ''} stroke-linejoin="round" stroke-linecap="round"/>`
    // while heating, a light pulse runs along the line instead of changing its weight
    if (hd)
      lines += `<path class="heatglow" d="${hd}" fill="none" stroke="${s.color}" stroke-width="6" stroke-linejoin="round" stroke-linecap="round"/><path class="heatline" d="${hd}" fill="none" stroke-width="2.2" stroke-linejoin="round" stroke-linecap="round"/>`
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
  const fmtT = (t: number) => {
    const d = new Date(t * 1000)
    const time = d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })
    return span > 86400 ? `${d.toLocaleDateString('es-ES', { weekday: 'short', day: 'numeric', month: 'short' })} · ${time}` : time
  }
  // the value the line shows at t: the last step for step lines, the interpolation for the others
  const valueAt = (s: ChartSeries, t: number): number | null => {
    const maxGap = s.gapSeconds ?? gap
    let prev: MetricPoint | null = null,
      next: MetricPoint | null = null
    for (const p of s.points) {
      if (p[1] === null) continue
      if (p[0] <= t) prev = p
      else {
        next = p
        break
      }
    }
    if (s.step || lanes) return prev && t - prev[0] <= maxGap ? prev[1] : null
    if (prev && next && next[0] - prev[0] <= maxGap)
      return prev[1]! + ((next[1]! - prev[1]!) * (t - prev[0])) / (next[0] - prev[0])
    const near = [prev, next].filter(isPresent).find((p) => Math.abs(p[0] - t) <= maxGap / 2)
    return near ? near[1] : null
  }
  const wrap = svg.parentElement
  let tip: HTMLElement
  const found = readout ?? wrap?.querySelector('.readout')
  if (found instanceof HTMLElement) tip = found
  else {
    tip = document.createElement('div')
    tip.className = 'readout'
    wrap?.insertBefore(tip, svg)
  }
  const last = Math.max(t0, ...visible.flatMap((s) => s.points.filter((p) => p[1] !== null).slice(-1).map((p) => p[0])))
  const head = (t: number | null) =>
    (label ? `<b class="rd-label">${esc(label)}</b>` : '') +
    (showTime ? `<b class="rd-time">${t === null ? 'Ahora' : fmtT(t)}</b>` : '')
  // with no cursor a value older than a quarter hour says when it is from
  const stale = t1 - last > 900 ? ` <em>a las ${fmtT(last)}</em>` : ''

  // draws the cursor at time t, whichever synced chart the pointer is on; with no cursor it reads the latest values
  const showAt = (cursor: number | null) => {
    const sc = svg.querySelector('.scrub')
    const line = sc?.firstElementChild
    const dots = sc?.querySelector('.dots')
    if (!(sc instanceof SVGGElement) || !(line instanceof SVGLineElement) || !dots) return
    const inside = cursor !== null && cursor >= t0 && cursor <= t1
    const t = inside ? cursor : last
    sc.style.display = inside ? '' : 'none'
    const px = String(x(t))
    line.setAttribute('x1', px)
    line.setAttribute('x2', px)
    if (floating) {
      // a phone shows it as a row above the plot, with the current values when nobody touches
      floating.hidden = !inside && !matchMedia('(max-width: 520px)').matches
      if (inside) {
        // at the far edge from the cursor, so it covers the distant past and not the moment being read
        const box = floating.offsetParent?.getBoundingClientRect()
        const rect = svg.getBoundingClientRect()
        if (box) {
          const right = rect.left - box.left + (x(t) / W) * rect.width < box.width / 2
          floating.style.left = right ? '' : '18px'
          floating.style.right = right ? '18px' : ''
        }
      }
    }
    if (lanes && !laneNames) {
      dots.innerHTML = ''
      tip.innerHTML = visible
        .filter((s) => s.readoutName)
        .map((s) => {
          const v = valueAt(s, t)
          return `<div class="rd-row"><i style="--c:${s.color}"></i><span>${esc(s.readoutName!)}</span><b>${v === null ? 'sin dato' : v >= 0.5 ? 'encendida' : 'apagada'}</b><em></em></div>`
        })
        .join('')
    } else if (lanes) {
      const rows = visible.map((s) => ({ s, v: valueAt(s, t) }))
      dots.innerHTML = ''
      const open = rows.filter(({ v }) => v !== null && v >= 0.5).length
      tip.innerHTML =
        head(inside ? t : null) +
        '<div class="rd-items">' +
        rows
          .map(
            ({ s, v }) =>
              `<span class="${v !== null && v >= 0.5 ? '' : 'off'}"><i style="--c:${s.color}"></i>${esc(s.short ?? s.name)}</span>`,
          )
          .join('') +
        (rows.every(({ v }) => v === null) ? '<em>sin dato</em>' : open ? '' : '<em>nada encendido</em>') +
        '</div>'
    } else {
      const rows = visible
        .filter((s) => !s.twinOf)
        .flatMap((s) => {
          const live = !inside && s.current !== undefined && s.current !== null
          const v = live ? s.current! : valueAt(s, t)
          if (v === null || v === undefined) return []
          const twin = s.twins?.find((tw) => !tw.hidden)
          const target = twin ? (!inside && twin.current != null ? twin.current : valueAt(twin, t)) : null
          return [{ s, v, target, live }]
        })
      dots.innerHTML = rows
        .flatMap(({ s, v, live }) =>
          !live && v >= lo && v <= hi ? `<circle class="dotc" cx="${px}" cy="${y(v)}" r="4.5" fill="${s.color}"/>` : [],
        )
        .join('')
      // in half degrees: the sensors are good to about ±1 °C, so tenths would be false precision
      const delta = (v: number, target: number) => {
        const d = Math.round((v - target) * 2) / 2
        return d === 0
          ? 'en objetivo'
          : `${Math.abs(d).toFixed(1).replace('.', ',')}° por ${d > 0 ? 'encima' : 'debajo'}`
      }
      const anyLive = rows.some((r) => r.live)
      if (!floating && tip.closest('.chart-tip')) {
        tip.innerHTML = rows
          .map(
            ({ s, v }) =>
              `<div class="rd-row"><i style="--c:${s.color}"></i><span>${esc(s.short ?? s.name)}</span><b>${fmtV(v)}</b><em></em></div>`,
          )
          .join('')
        return
      }
      if (floating) {
        // one line per room: temperature, its target and whether its valve is open
        tip.innerHTML =
          `<div class="rd-head">${inside ? fmtT(t) : 'Ahora'}</div>` +
          rows
            .map(({ s, v, target }) => {
              const heating = s.heat?.some((h) => (valueAt({ ...s, step: true, points: h }, t) ?? 0) >= 0.5)
              return `<div class="rd-row"><i style="--c:${s.color}"></i><span>${esc(s.short ?? s.name)}${heating ? `<span class="rd-heat" title="Calentando">${iconHtml('flame')}</span>` : ''}</span><b>${fmtV(v)}</b><em${target === null ? ' title="Sin objetivo guardado en este momento"' : ''}>${target === null ? '/ —' : `/ ${fmtV(target).replace(' °C', '')}`}</em></div>`
            })
            .join('')
        return
      }
      tip.innerHTML =
        head(inside ? t : null) +
        '<div class="rd-items">' +
        (rows.length
          ? rows
              .map(
                ({ s, v, target }) =>
                  `<span><i style="--c:${s.color}"></i>${esc(s.short ?? s.name)} <b>${fmtV(v)}</b>${target === null ? (s.note && !inside ? `<em>${esc(s.note)}</em>` : '') : `<em>obj. ${fmtV(target).replace(' °C', '')} · ${delta(v, target)}</em>`}</span>`,
              )
              .join('') + (inside || anyLive ? '' : stale)
          : '<span class="muted">sin dato</span>') +
        '</div>'
    }
  }

  if (sync) join(sync, svg, showAt)
  const emit = (t: number | null) => (sync ? broadcast(sync, t) : showAt(t))
  const move = (ev: PointerEvent) => {
    const rect = svg.getBoundingClientRect(),
      px = ((ev.clientX - rect.left) / rect.width) * W
    if (px < P.l || px > W - P.r) return
    emit(t0 + ((px - P.l) / (W - P.l - P.r)) * (t1 - t0))
  }
  svg.onpointermove = move
  svg.onpointerdown = move
  svg.onpointerleave = () => emit(null)
  showAt(sync ? (cursors.get(sync) ?? null) : null)
  const sinceTxt = new Date(first * 1000).toLocaleTimeString('es-ES', {
    hour: '2-digit',
    minute: '2-digit',
  })
  const note = partial
    ? `<span class="chart-note">Solo hay datos desde las ${sinceTxt}${new Date(first * 1000).toLocaleDateString() !== new Date(now * 1000).toLocaleDateString() ? ' del ' + new Date(first * 1000).toLocaleDateString('es-ES', { day: 'numeric', month: 'short' }) : ''}</span>`
    : ''
  renderLegend(note)
}

/** Whether any of the on/off series is on at ts; ts must not decrease between calls. */
function heatTimeline(series: MetricPoint[][], maxAge: number): (ts: number) => boolean {
  const at = series.map(() => -1)
  return (ts) =>
    series.some((points, k) => {
      while (at[k] + 1 < points.length && points[at[k] + 1][0] <= ts) at[k]++
      const p = points[at[k]]
      return p !== undefined && p[1] !== null && p[1] >= 0.5 && ts - p[0] <= maxAge
    })
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
