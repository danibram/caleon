import 'react'

// stagger delays and icon masks are passed as custom properties on `style`
declare module 'react' {
  interface CSSProperties {
    [custom: `--${string}`]: string | number | undefined
  }
}
