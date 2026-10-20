import { lazy, Suspense, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client.ts'
import type { WeatherView } from '../api/generated/WeatherView.ts'
import type { Selection } from '../api/generated/Selection.ts'
import type { Location } from '../api/generated/Location.ts'
import type { StationCandidate } from '../api/generated/StationCandidate.ts'
import { errorMessage } from '../lib/format.ts'
import { weatherQuery } from '../lib/weather.ts'

const WeatherMap = lazy(() => import('./WeatherMap.tsx'))

function distance(a: Location, station: StationCandidate) {
  const lat = (a.coordinates.latitude * Math.PI) / 180
  const dy = (station.station.coordinates.latitude - a.coordinates.latitude) * 111.2
  const dx = (station.station.coordinates.longitude - a.coordinates.longitude) * 111.2 * Math.cos(lat)
  return Math.hypot(dx, dy).toLocaleString('es-ES', {
    maximumFractionDigits: 1,
  })
}

export function WeatherSettings() {
  const { data, error: loadError } = useQuery(weatherQuery)
  const cache = useQueryClient()
  const [editing, setEditing] = useState(false)
  const [selection, setSelection] = useState<Selection | null>(null)
  const [provider, setProvider] = useState<'open-meteo' | 'aemet'>('open-meteo')
  const [key, setKey] = useState('')
  const [search, setSearch] = useState('')
  const [locations, setLocations] = useState<Location[]>([])
  const [stations, setStations] = useState<StationCandidate[]>([])
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  function begin() {
    setSelection(data?.settings.selection ?? null)
    setProvider(data?.settings.selection?.target.provider ?? 'open-meteo')
    setKey('')
    setStations([])
    setLocations([])
    setSearch('')
    setError(null)
    setEditing(true)
  }

  function chooseLocation(location: Location) {
    setSelection({ location, target: { provider: 'open-meteo' } })
    setStations([])
    setLocations([])
    setError(null)
  }

  async function recover() {
    setBusy('recover')
    setError(null)
    try {
      await api('/api/weather/recover', { body: {} })
      await cache.invalidateQueries({ queryKey: ['weather'] })
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setBusy(null)
    }
  }

  async function apply(enabled: boolean) {
    setBusy('save')
    setError(null)
    try {
      const updated = await api<WeatherView>('/api/weather', {
        body: { enabled, selection, aemet_api_key: provider === 'aemet' && key ? key : undefined },
      })
      cache.setQueryData(['weather'], updated)
      void cache.invalidateQueries({ queryKey: ['weather-history'] })
      setKey('')
      setEditing(false)
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setBusy(null)
    }
  }

  async function findLocations() {
    setBusy('locations')
    setError(null)
    try {
      setLocations(await api<Location[]>(`/api/weather/locations?name=${encodeURIComponent(search.trim())}`))
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setBusy(null)
    }
  }

  async function findStations() {
    if (!selection) return
    setBusy('stations')
    setError(null)
    try {
      const candidates = await api<StationCandidate[]>('/api/weather/stations', {
        body: {
          location: selection.location.coordinates,
          aemet_api_key: key || undefined,
        },
      })
      setStations(candidates)
      const nearest = candidates[0]
      if (nearest)
        setSelection({
          ...selection,
          target: { provider: 'aemet', station: nearest.station },
        })
      else setError('No hay estaciones con temperatura reciente para esta consulta.')
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setBusy(null)
    }
  }

  const station = selection?.target.provider === 'aemet' ? selection.target.station : null
  const canSave =
    selection &&
    (provider === 'open-meteo' || (station && (key.trim() || data?.settings.aemet_key_configured)))

  return (
    <>
      <h2 className="group-title">Temperatura exterior</h2>
      <section className="card weather-settings" aria-label="Configuración de temperatura exterior">
        <div className="spread">
          <div>
            <b>Comparar con el exterior</b>
            <p className="muted small">Relaciona las temperaturas de casa con las de tu zona.</p>
          </div>
          <button
            type="button"
            role="switch"
            aria-label="Activar temperatura exterior"
            aria-checked={data?.settings.enabled ?? false}
            className="weather-switch"
            disabled={!data || busy !== null || editing}
            onClick={() => (data?.settings.enabled ? void apply(false) : begin())}
          >
            <span />
          </button>
        </div>
        {loadError ? (
          <p className="banner" role="alert">
            No se pudieron leer los ajustes meteorológicos.
          </p>
        ) : null}
        {data && !data.history_enabled ? (
          <p className="banner">
            El servidor tiene el histórico deshabilitado. Puedes configurar la ubicación, pero no recogerá
            datos exteriores.
          </p>
        ) : null}
        {data?.status.error ? (
          <p className="banner" role="status">
            {data?.status.error}
            {data?.status.effective_provider === 'open-meteo' &&
            data?.settings.selection?.target.provider === 'aemet'
              ? ' · Usando Open-Meteo temporalmente.'
              : ''}
          </p>
        ) : null}
        {data?.settings.enabled && !editing ? (
          <>
            <div className="weather-summary">
              <b>{data.settings.selection?.location.name}</b>
              <span className="muted small">
                {data.settings.selection?.target.provider === 'aemet'
                  ? `AEMET · ${data.settings.selection.target.station.name}`
                  : 'Open-Meteo · temperatura estimada'}
              </span>
            </div>
            <button type="button" className="btn" disabled={busy !== null} onClick={begin}>
              Cambiar ubicación o proveedor
            </button>
            <div className="weather-recovery">
              <b>Huecos en el histórico</b>
              <p className="muted small">
                Si el servidor estuvo apagado o el proveedor no respondió, puedes pedir las horas que faltan de los
                últimos 30 días.
              </p>
              <button
                type="button"
                className="btn"
                disabled={busy !== null || data.status.recovering || !data.history_enabled}
                onClick={() => void recover()}
              >
                {busy === 'recover' || data.status.recovering ? 'Recuperando huecos…' : 'Recuperar huecos'}
              </button>
              {data.status.recovery ? (
                <p className="muted small" role="status">
                  Última recuperación: {data.status.recovery.recovered_hours} horas recuperadas
                  {data.status.recovery.remaining_hours
                    ? ` · ${data.status.recovery.remaining_hours} horas sin datos en el proveedor`
                    : ' · no falta ninguna hora'}
                  {data.status.recovery.failed_queries ? ` · ${data.status.recovery.failed_queries} consultas fallidas` : ''}
                </p>
              ) : null}
            </div>
          </>
        ) : null}
        {!data?.settings.enabled && !editing ? (
          <p className="muted small">
            Desactivado. No se consultan proveedores. La configuración y el histórico se conservan.
          </p>
        ) : null}
        {editing ? (
          <div className="weather-editor">
            <fieldset className="weather-providers">
              <legend>Proveedor</legend>
              <label>
                <input
                  type="radio"
                  name="weather-provider"
                  disabled={busy !== null}
                  checked={provider === 'open-meteo'}
                  onChange={() => {
                    setProvider('open-meteo')
                    setError(null)
                    if (selection)
                      setSelection({
                        ...selection,
                        target: { provider: 'open-meteo' },
                      })
                  }}
                />{' '}
                Open-Meteo <span className="muted small">Sin clave · estimación</span>
              </label>
              <label>
                <input
                  type="radio"
                  name="weather-provider"
                  disabled={busy !== null}
                  checked={provider === 'aemet'}
                  onChange={() => {
                    setProvider('aemet')
                    setError(null)
                  }}
                />{' '}
                AEMET <span className="muted small">Con clave · estación</span>
              </label>
            </fieldset>
            <form
              className="weather-search"
              onSubmit={(event) => {
                event.preventDefault()
                void findLocations()
              }}
            >
              <label className="field">
                <span>Localidad o código postal</span>
                <input
                  disabled={busy !== null}
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Buscar la ubicación de la vivienda"
                  autoComplete="off"
                />
              </label>
              <button type="submit" className="btn" disabled={search.trim().length < 2 || busy !== null}>
                {busy === 'locations' ? 'Buscando…' : 'Buscar'}
              </button>
            </form>
            {locations.length ? (
              <ul className="weather-results">
                {locations.map((location) => (
                  <li key={`${location.coordinates.latitude},${location.coordinates.longitude}`}>
                    <button type="button" disabled={busy !== null} onClick={() => chooseLocation(location)}>
                      {location.name}
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
            <p className="muted small">Busca una localidad o toca el mapa para situar la vivienda.</p>
            <Suspense fallback={<div className="weather-map weather-map-loading">Cargando mapa…</div>}>
              <WeatherMap
                location={selection?.location.coordinates ?? null}
                stations={provider === 'aemet' ? stations : []}
                selectedStation={station?.id ?? null}
                onPick={(coordinates) =>
                  !busy &&
                  chooseLocation({
                    name: 'Ubicación seleccionada',
                    coordinates,
                  })
                }
                onStation={(candidate) =>
                  !busy &&
                  selection &&
                  setSelection({
                    ...selection,
                    target: { provider: 'aemet', station: candidate.station },
                  })
                }
              />
            </Suspense>
            {selection ? (
              <p className="small weather-location">
                <b>{selection.location.name}</b>
                <span className="muted">
                  {selection.location.coordinates.latitude.toFixed(4)},{' '}
                  {selection.location.coordinates.longitude.toFixed(4)}
                </span>
              </p>
            ) : null}
            {provider === 'aemet' ? (
              <>
                <label className="field">
                  <span>Clave de AEMET</span>
                  <input
                    type="password"
                    disabled={busy !== null}
                    value={key}
                    onChange={(event) => setKey(event.target.value)}
                    placeholder={
                      data?.settings.aemet_key_configured
                        ? 'Clave guardada. Escribe para sustituirla'
                        : 'Pega tu API key'
                    }
                    autoComplete="off"
                  />
                </label>
                <a
                  className="weather-key-link"
                  href={data?.aemet_api_key_url ?? 'https://opendata.aemet.es/centrodedescargas/altaUsuario'}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Obtener clave de AEMET ↗
                </a>
                <p className="muted small">
                  AEMET te enviará la clave por correo. Si deja de responder, Open-Meteo cubrirá temporalmente
                  la ubicación de la vivienda.
                </p>
                <button
                  type="button"
                  className="btn"
                  disabled={
                    !selection || (!key.trim() && !data?.settings.aemet_key_configured) || busy !== null
                  }
                  onClick={() => void findStations()}
                >
                  {busy === 'stations' ? 'Buscando estaciones…' : 'Buscar estaciones cercanas'}
                </button>
                {stations.length && selection ? (
                  <fieldset className="weather-stations">
                    <legend>Confirma la estación preseleccionada o elige otra</legend>
                    {stations.slice(0, 12).map((candidate) => (
                      <label key={candidate.station.id}>
                        <input
                          type="radio"
                          name="weather-station"
                          disabled={busy !== null}
                          checked={station?.id === candidate.station.id}
                          onChange={() =>
                            setSelection({
                              ...selection,
                              target: {
                                provider: 'aemet',
                                station: candidate.station,
                              },
                            })
                          }
                        />
                        <span>
                          <b>{candidate.station.name}</b>
                          <small className="muted">
                            {distance(selection.location, candidate)} km ·{' '}
                            {candidate.station.elevation_m ?? '?'} m ·{' '}
                            {candidate.latest_temperature
                              ? `${candidate.latest_temperature.temperature_c.toLocaleString('es-ES')} °C · ${new Date(candidate.latest_temperature.ts * 1000).toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' })}`
                              : 'Sin lectura reciente'}
                          </small>
                        </span>
                      </label>
                    ))}
                  </fieldset>
                ) : null}
                {station ? <p className="small">Estación elegida · {station.name}</p> : null}
              </>
            ) : null}
            <div className="actions weather-editor-actions">
              <button
                type="button"
                className="btn primary"
                disabled={!canSave || busy !== null}
                onClick={() => void apply(true)}
              >
                {busy === 'save' ? 'Comprobando…' : 'Guardar y activar'}
              </button>
              <button
                type="button"
                className="btn"
                disabled={busy !== null}
                onClick={() => {
                  setEditing(false)
                  setKey('')
                  setError(null)
                }}
              >
                Cancelar
              </button>
            </div>
          </div>
        ) : null}
        {error ? (
          <p className="banner" role="alert">
            {error}
          </p>
        ) : null}
      </section>
    </>
  )
}
