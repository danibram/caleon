import { createHashRouter } from 'react-router'
import { NotFound } from './views/NotFound.tsx'
import { AppShell } from './components/AppShell.tsx'
import { StateGate } from './components/StateGate.tsx'
import { ChartsView } from './charts/ChartsView.tsx'
import MenuView from './views/MenuView.tsx'
import { OverviewView } from './views/OverviewView.tsx'
import { RoomView } from './views/RoomView.tsx'
import { TimerView } from './views/TimerView.tsx'
import { StatusView } from './views/StatusView.tsx'
import { BackupsView } from './views/BackupsView.tsx'
import { InfoView } from './views/InfoView.tsx'
import { SettingsView } from './views/SettingsView.tsx'

export const router = createHashRouter([
  {
    path: '/',
    element: <AppShell />,
    errorElement: <NotFound />,
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
            <ChartsView />
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

