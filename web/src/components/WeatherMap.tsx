import { useEffect, useEffectEvent, useRef } from 'react'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import type { Coordinates } from '../api/generated/Coordinates.ts'
import type { StationCandidate } from '../api/generated/StationCandidate.ts'

interface Props {
  location: Coordinates | null
  stations: StationCandidate[]
  selectedStation: string | null
  onPick: (location: Coordinates) => void
  onStation: (station: StationCandidate) => void
}

export default function WeatherMap({ location, stations, selectedStation, onPick, onStation }: Props) {
  const host = useRef<HTMLDivElement>(null)
  const map = useRef<L.Map | null>(null)
  const markers = useRef<L.LayerGroup | null>(null)
  const pick = useEffectEvent(onPick)
  const choose = useEffectEvent(onStation)

  useEffect(() => {
    if (!host.current) return
    const instance = L.map(host.current).setView([40.4, -3.7], 5)
    L.tileLayer(import.meta.env.VITE_WEATHER_TILE_URL || 'https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
      maxZoom: 18,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
    }).addTo(instance)
    instance.on('click', (event: L.LeafletMouseEvent) => {
      const point = event.latlng.wrap()
      pick({ latitude: point.lat, longitude: point.lng })
    })
    map.current = instance
    markers.current = L.layerGroup().addTo(instance)
    return () => {
      instance.remove()
      map.current = null
      markers.current = null
    }
  }, [])

  useEffect(() => {
    const layer = markers.current
    if (!map.current || !layer) return
    layer.clearLayers()
    if (location) {
      L.circleMarker([location.latitude, location.longitude], {
        radius: 8,
        color: '#b74721',
        fillColor: '#ed7144',
        fillOpacity: 1,
        weight: 3,
      })
        .bindTooltip('Vivienda')
        .addTo(layer)
    }
    for (const candidate of stations) {
      const station = candidate.station
      const label = document.createElement('span')
      label.textContent = station.name
      L.circleMarker([station.coordinates.latitude, station.coordinates.longitude], {
        radius: station.id === selectedStation ? 9 : 5,
        color: '#246b88',
        fillOpacity: 0.85,
        weight: 2,
        bubblingMouseEvents: false,
      })
        .bindTooltip(label)
        .on('click', () => choose(candidate))
        .addTo(layer)
    }
  }, [location, stations, selectedStation])

  useEffect(() => {
    if (location && map.current) {
      map.current.setView([location.latitude, location.longitude], Math.max(map.current.getZoom(), 10))
    }
  }, [location])

  return (
    <div
      ref={host}
      className="weather-map"
      role="region"
      aria-label="Mapa de vivienda y estaciones meteorológicas"
    />
  )
}
