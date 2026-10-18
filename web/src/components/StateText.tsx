import type { StateLine } from '../lib/rooms.ts'
import { Icon } from './Icon.tsx'

/** "A temperatura · Normal ⤒ 21,5°": what the room does, then the mode it follows and the
 *  setpoint as one unit, so a narrow card wraps the mode and setpoint together to a second line. */
export function StateText({ st }: { st: StateLine }) {
  return (
    <>
      <span className="st-what">{st.text}</span>
      {st.mode || st.target ? (
        <span className="st-how">
          {st.mode}
          {st.target ? (
            <>
              <span className="target" title="Temperatura objetivo" aria-label={`objetivo ${st.target}`}>
                <Icon name="target" className="mini" />
                {st.target}
              </span>
            </>
          ) : null}
        </span>
      ) : null}
    </>
  )
}
