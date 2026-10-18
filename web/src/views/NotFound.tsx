import { Link } from 'react-router'
/** Unknown links and render errors land on a page in the app's own style, not the router's default. */
export function NotFound() {
  return (
    <main id="view">
      <div className="empty">
        Esta página no existe. <Link to="/">Volver al inicio</Link>
      </div>
    </main>
  )
}
