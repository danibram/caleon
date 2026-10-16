import { Link } from 'react-router'
import { Icon, type IconName } from './Icon.tsx'

export type ActionItem = {
  tag: 'a' | 'button'
  href?: string
  id?: string
  icon: IconName
  label: string
  sub?: string
  onClick?: () => void
  disabled?: boolean
}

export function ActionRow({ items }: { items: ActionItem[] }) {
  return (
    <div className="actrow stagger">
      {items.map(({ tag, href, id, icon, label, sub, onClick, disabled }, k) => {
        const inner = (
          <>
            <span className="ic">
              <Icon name={icon} />
            </span>
            <span className="lbl">
              {label}
              {sub ? <span className="sub">{sub}</span> : null}
            </span>
            <Icon name="chevron" className="chev" />
          </>
        )
        const style = { ['--k' as string]: k }
        if (tag === 'a' && href) {
          return (
            <Link key={href + label} to={href.replace(/^#/, '')} style={style}>
              {inner}
            </Link>
          )
        }
        return (
          <button
            key={id || label}
            type="button"
            id={id}
            style={style}
            disabled={disabled}
            onClick={onClick}
          >
            {inner}
          </button>
        )
      })}
    </div>
  )
}
