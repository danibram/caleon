# Web UI

React + TypeScript + Vite front end of the caleon server. The Rust binary embeds `dist/`, so a
release build needs `pnpm build` first (`build.rs` writes a placeholder page when `dist/` is
missing, so `cargo build` still works).

```sh
pnpm install
pnpm dev                 # proxies /api and /icons to http://127.0.0.1:8088
CALEON_API=http://127.0.0.1:8089 pnpm dev   # another server
VITE_MOCK=1 pnpm dev     # no server, no thermostat: fixtures from src/mocks
pnpm lint
pnpm build
```

TypeScript types for the API come from the Rust structs. `cargo test export_types` writes
`src/api/generated/`. See the [main README](../README.md) for the server and the API.
