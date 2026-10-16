import type { ReactNode } from 'react'
import { createHashRouter } from 'react-router'
import { AppShell } from './components/AppShell.tsx'
import { ChartsView } from './charts/ChartsView.tsx'
import { useApp } from './context/AppContext.tsx'
import MenuView from './views/MenuView.tsx'
import { OverviewView } from './views/OverviewView.tsx'
import { RoomView } from './views/RoomView.tsx'
import { TimerView } from './views/TimerView.tsx'
import { StatusView } from './views/StatusView.tsx'
import { BackupsView } from './views/BackupsView.tsx'
import { InfoView } from './views/InfoView.tsx'
import { SettingsView } from './views/SettingsView.tsx'

function ChartsRoute() {
  const { S } = useApp()
  if (!S) return null
  return <ChartsView state={S} />
}

function StateGate({ children }: { children: ReactNode }) {
  const { S } = useApp()
  if (!S) {
    return (
      <div className="empty">
        <span className="spin" /> Conectando con el termostato…
      </div>
    )
  }
  return children
}

export const router = createHashRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      {
        index: true,
        element: (
          <StateGate>
            <OverviewView />
          </StateGate>
        ),
      },
      {
        path: 'room/:id',
        element: (
          <StateGate>
            <RoomView />
          </StateGate>
        ),
      },
      {
        path: 'timer/:id',
        element: (
          <StateGate>
            <TimerView />
          </StateGate>
        ),
      },
      { path: 'status', element: <StatusView /> },
      {
        path: 'charts',
        element: (
          <StateGate>
            <ChartsRoute />
          </StateGate>
        ),
      },
      {
        path: 'menu/*',
        element: (
          <StateGate>
            <MenuView />
          </StateGate>
        ),
      },
      { path: 'backups', element: <BackupsView /> },
      { path: 'info', element: <InfoView /> },
      { path: 'settings', element: <SettingsView /> },
    ],
  },
])
