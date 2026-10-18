import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App.tsx'
import { AppProvider } from './context/AppProvider.tsx'
import { applyTheme } from './lib/theme.ts'
import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import './index.css'

applyTheme()

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 5000 },
  },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <AppProvider>
        <App />
      </AppProvider>
    </QueryClientProvider>
  </StrictMode>,
)
