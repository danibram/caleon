import { isRecord } from '../../lib/guards'

/** The screen the thermostat is showing; `id` is empty once the menu has been closed. */
export type MenuScreen = {
  id: string
  title: string
  type: string
  back?: string
  back_hold?: string
  hold_action?: string
  forward_hold?: string
  forward?: string
}

/** POST /api/menu/action and /api/menu/open: `detail` is whatever the device sends for `menu.type`. */
export type MenuResponse = { menu: MenuScreen; detail: unknown }

export type StringListItem = { title: string; action?: string; hold_action?: string }

export type TileItem = {
  title: string
  icon: string
  icon_text: string
  highlighted: boolean
  action?: string
  hold_action?: string
}

export type OffsetRow = {
  icon: string
  icon_text: string
  value: string
  offset: string
  decrease?: string
  increase?: string
}

export type InfoLine = { text: string; action?: string }

export type TimeRange = { start: number; stop: number; mode: number; title: string; hint: string }

export type ClockFields = { year: number; month: number; day: number; hour: number; minute: number }

export type MenuBody =
  | { kind: 'string list'; items: StringListItem[] }
  | { kind: 'tiles'; hint: string; items: TileItem[] }
  | { kind: 'options'; hint: string; value: string; decrease?: string; increase?: string }
  | { kind: 'offset'; hint: string; rows: OffsetRow[] }
  | { kind: 'information'; lines: InfoLine[] }
  | { kind: 'binary query'; lines: string[]; value: string; yes: string; no: string }
  | { kind: 'date'; yearless: boolean; hint: string; clock: ClockFields }
  | { kind: 'time'; clock: ClockFields }
  | { kind: 'datetime'; clock: ClockFields }
  | { kind: 'text'; value: string }
  | { kind: 'timer'; words: number[] }
  | { kind: 'time ranges'; withStop: boolean; day: number; modes: Record<number, string>; ranges: TimeRange[] }
  | { kind: 'raw'; detail: unknown }

const str = (v: unknown): string => (v == null ? '' : String(v))

// an empty action means the device offers nothing to press
const action = (v: unknown): string | undefined => (v ? String(v) : undefined)

const records = (v: unknown): Record<string, unknown>[] => (Array.isArray(v) ? v.filter(isRecord) : [])

const clockOf = (d: Record<string, unknown>): ClockFields => ({
  year: Number(d.year),
  month: Number(d.month),
  day: Number(d.day),
  hour: Number(d.hour),
  minute: Number(d.minute),
})

/** Shapes the device's loosely typed detail into the body of the screen type it came with. */
export function parseMenuBody(type: string, detail: unknown): MenuBody {
  const d = isRecord(detail) ? detail : {}
  switch (type) {
    case 'string list':
      return {
        kind: 'string list',
        items: records(d.menu).map((i) => ({
          title: str(i.title),
          action: action(i.action),
          hold_action: action(i.hold_action),
        })),
      }
    case 'dynamic list':
    case 'dynamic list tiled':
      return {
        kind: 'tiles',
        hint: str(d.hint),
        items: records(d.menu).map((i) => ({
          title: str(i.title),
          icon: str(i.icon),
          icon_text: str(i.icon_text),
          highlighted: !!i.highlighted,
          action: action(i.action),
          hold_action: action(i.hold_action),
        })),
      }
    case 'options':
      return {
        kind: 'options',
        hint: str(d.hint),
        value: str(d.value),
        decrease: action(d.decrease),
        increase: action(d.increase),
      }
    case 'offset':
      return {
        kind: 'offset',
        hint: str(d.hint),
        rows: records(d.menu).map((i) => ({
          icon: str(i.icon),
          icon_text: str(i.icon_text),
          value: str(i.value),
          offset: str(i.offset),
          decrease: action(i.decrease),
          increase: action(i.increase),
        })),
      }
    case 'information':
      return {
        kind: 'information',
        lines: records(d.menu).map((l) => ({ text: str(l.text), action: action(l.action) })),
      }
    case 'binary query':
      return {
        kind: 'binary query',
        lines: records(d.hint).map((l) => str(l.line)),
        value: str(d.value),
        yes: str(d.yes),
        no: str(d.no),
      }
    case 'system date':
    case 'month day':
      return { kind: 'date', yearless: type === 'month day', hint: str(d.hint), clock: clockOf(d) }
    case 'system time':
      return { kind: 'time', clock: clockOf(d) }
    case 'system datetime':
      return { kind: 'datetime', clock: clockOf(d) }
    case 'text':
      return { kind: 'text', value: str(d.value) }
    case 'timer':
      return { kind: 'timer', words: records(d.timer).map((w) => Number(w.value) || 0) }
    case 'time ranges':
    case 'time ranges 2':
      return {
        kind: 'time ranges',
        withStop: type === 'time ranges',
        day: Number(d.day),
        modes: Object.fromEntries(records(d.modes).map((x) => [Number(x.mode_id), str(x.mode_text)])),
        ranges: records(d.ranges).map((x) => ({
          start: Number(x.start),
          stop: Number(x.stop),
          mode: Number(x.mode),
          title: str(x.title),
          hint: str(x.hint),
        })),
      }
    default:
      return { kind: 'raw', detail }
  }
}
