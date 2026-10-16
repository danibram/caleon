import { historySpan } from './chartData.ts'
import type { StateSnapshot } from '../api/types.ts'

const RANGE_LABELS: Record<number, string> = {
  6: '6 h',
  24: '24 h',
  168: '7 días',
  720: '30 días',
}

interface RangeSegProps {
  id: string
  ranges: number[]
  current: number
  state: StateSnapshot
  onChange: (hours: number) => void
}

export function RangeSeg({ id, ranges, current, state, onChange }: RangeSegProps) {
  const span = historySpan(state)
  const ok = (h: number, i: number) => i === 0 || span >= h * 3600 * 0.3
  const someDisabled = ranges.some((h, i) => !ok(h, i))
  const title = someDisabled
    ? 'Los rangos largos se activan cuando haya más historial guardado'
    : undefined

  return (
    <div className="seg" id={id} title={title}>
      {ranges.map((h, i) => (
        <button
          key={h}
          type="button"
          data-h={h}
          className={h === current ? 'active' : ''}
          disabled={!ok(h, i)}
          title={!ok(h, i) ? `Aún no hay ${RANGE_LABELS[h]} de datos` : undefined}
          onClick={() => {
            if (!ok(h, i)) return
            onChange(h)
          }}
        >
          {RANGE_LABELS[h]}
        </button>
      ))}
    </div>
  )
}
