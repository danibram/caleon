import { api } from '../api/client.ts'
import type { WeatherView } from '../api/generated/WeatherView.ts'

export const weatherQuery = {
  queryKey: ['weather'],
  queryFn: () => api<WeatherView>('/api/weather'),
  refetchInterval: 15000,
}

const providerNames = new Map([
  ['aemet', 'AEMET'],
  ['open-meteo', 'Open-Meteo'],
])
export const weatherProviderName = (provider: string) => providerNames.get(provider) ?? provider
