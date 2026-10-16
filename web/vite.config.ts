import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import { mockApi } from './mock-api.ts'

const mock = process.env.VITE_MOCK === '1'
// point this at a server started with READ_ONLY=1 while developing
const api = process.env.CALEON_API ?? 'http://127.0.0.1:8088'

export default defineConfig({
  plugins: [react(), tailwindcss(), ...(mock ? [mockApi()] : [])],
  server: {
    proxy: mock
      ? undefined
      : {
          '/api': api,
          '/icons': api,
        },
  },
})
