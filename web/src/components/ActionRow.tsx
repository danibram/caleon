import { Link } from 'react-router'
import { Icon, type IconName } from './Icon.tsx'

type ActionBase = {
  icon: IconName
  label: string
  sub?: string
}

export type ActionItem =
  | (ActionBase & { tag: 'a'; href: string })
  | (ActionBase & { tag: 'button'; id?: string; onClick?: () => void; disabled?: boolean })

export function ActionRow({ items }: { items: ActionItem[] }) {
  return (
    <div className="actrow stagger">
      {items.map((item, k) => {
        const inner = (
          <>
            <span className="ic">
              <Icon name={item.icon} />
            </span>
            <span className="lbl">
              {item.label}
              {item.sub ? <span className="sub">{item.sub}</span> : null}
            </span>
            <Icon name="chevron" className="chev" />
          </>
        )
        const style = { '--k': k }
        if (item.tag === 'a') {
          return (
            <Link key={item.href + item.label} to={item.href.replace(/^#/, '')} style={style}>
              {inner}
            </Link>
          )
        }
        return (
          <button
            key={item.id || item.label}
            type="button"
            id={item.id}
            style={style}
            disabled={item.disabled}
            onClick={item.onClick}
          >
            {inner}
          </button>
        )
      })}
    </div>
  )
}
