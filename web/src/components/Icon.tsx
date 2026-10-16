import { IC, type IconName } from '../lib/iconPaths.ts'

export type { IconName }

export function Icon({ name, className = '' }: { name: IconName; className?: string }) {
  const path = IC[name]
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      dangerouslySetInnerHTML={{ __html: path }}
    />
  )
}
