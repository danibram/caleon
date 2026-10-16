import { ROOM_TYPES } from '../../lib/constants'
import { es as esFormat } from '../../lib/format'

export { esFormat as es }
import type { IconName } from '../../lib/iconPaths'
import { roomTitle, sensorRoom, typeLabel, zoneRooms } from '../../lib/rooms'
import type { Snapshot } from '../../context/AppContext'

export const PRETTY: Record<string, string> = {
  'Valores fábrica': 'Valores de fábrica',
  'Protec. antibloqueo': 'Protección antibloqueo',
  'Fecha & Hora': 'Fecha y hora',
  'Hora & Fecha': 'Fecha y hora',
  'Retardo activ.': 'Retardo de encendido',
  'Retardo desactiv.': 'Retardo de apagado',
  'Sincron. habitación': 'Sincronizar habitaciones',
  'Interv. de transm.': 'Intervalo de envío',
  'Día V/I': 'Cambio verano/invierno (normal)',
  'Eco V/I': 'Cambio verano/invierno (eco)',
  'Funciones °Cbox': 'Funciones de la centralita',
  'Circ.Calefacción': 'Circuito de calefacción',
  'Conmut. estacional 2': 'Conmutación estacional 2',
  'Estacional estacional': 'Señal estacional',
  Experto: 'Experto',
  Ajustes: 'Configuración del equipo',
  Estado: 'Valores del equipo',
  'Valores de servicio': 'Valores del equipo',
}

export const unent = (t: string | null | undefined) => {
  const d = document.createElement('textarea')
  d.innerHTML = t ?? ''
  return d.value
}

export const prettyTitle = (t: string | null | undefined) => {
  const u = unent(t)
  return PRETTY[u] || u
}

export const onOff = (t: string) =>
  String(t)
    .replace(/: ?On$/i, ': Encendido')
    .replace(/: ?Off$/i, ': Apagado')
    .replace(/^On$/i, 'Encendido')
    .replace(/^Off$/i, 'Apagado')

export type MenuTrailItem = { id: string; title: string; entry: string }

export const entryPath = (menuTrail: MenuTrailItem[]) =>
  menuTrail.slice(1).map((x) => x.entry)

export const MENU_ICON: Record<string, IconName> = {
  Idioma: 'globe',
  'Fecha & Hora': 'clock',
  Ajustes: 'gear',
  'Valores de servicio': 'pulse',
  'Bloqueo de menú': 'lock',
  'Valores fábrica': 'warn',
  Zonas: 'zone',
  WiFi: 'wifi',
  Habitaciones: 'home',
  'Gestión de dispositivos': 'chip',
  'Funciones °Cbox': 'sliders',
  Luminosidad: 'sun',
  'Modo de interfaz': 'layout',
  'Sincron. habitación': 'swap',
  'Sensor de WiFi': 'wifi',
  Habitación: 'home',
  'Día V/I': 'sun',
  'Eco V/I': 'leaf',
  'Modo de relé': 'swap',
  'Sensor de suelo': 'thermo',
  'Protec. antibloqueo': 'shield',
  Salida: 'chip',
  'Retardo activ.': 'clock',
  'Retardo desactiv.': 'clock',
  'Influencia habitación': 'home',
  'Hora de verano': 'sun',
  'Sincronización horaria': 'globe',
  Fecha: 'clock',
  Hora: 'clock',
  'Activar WiFi': 'wifi',
  'Estado WiFi': 'wifi',
  'Control de acceso': 'lock',
  SSID: 'wifi',
  Contraseña: 'lock',
}

export const MENU_DANGER = /fábrica|fabrica/i

export const INFO_HEAD: Record<string, string> = {
  Entradas: 'Sensores',
  Salidas: 'Relés',
  'Privat CAN': 'Red interna (CAN)',
  'Fecha y hora': 'Fecha y hora',
  'Bomba circuito 116.1': 'Bomba del circuito',
}

export function infoLabel({
  label,
  value,
  S,
}: {
  label: string
  value: string
  S: Snapshot | null
}): [string, string] {
  const zr = zoneRooms(S)
  let l = label
  let v = value
  const sr = sensorRoom(l, S)
  if (sr) l = `${/HR|%/.test(v) ? 'Humedad' : 'Sensor'} de ${roomTitle(sr)}`
  else if (l === '1')
    l = /HR|%/.test(v) ? 'Humedad del termostato' : 'Sensor del termostato'
  else if (/^\d+\.\d+$/.test(l)) l = `Sonda ${l}`
  else if (l === 'Ida nominal') l = 'Temperatura objetivo del agua'
  else if (l === 'Ida') l = 'Temperatura del agua'
  else if (l === 'Modo') l = 'Modo de la bomba'
  else if (/^V$/.test(l)) l = 'Versión'
  else if (/^\d+\.N\d+$/.test(l)) l = `Entrada auxiliar ${l.split('.')[1]}`
  else {
    const z = /^\d+\.([A-Z]) Zona ([A-Z])$/.exec(l)
    if (z)
      l = `Zona ${z[2]}${zr[z[2]] ? ' · ' + roomTitle(zr[z[2]]) : ' · sin habitación'}`
    else if (/^\d+\.[A-Z0-9]+ --$/.test(l))
      l = `Salida ${l.split(' ')[0].split('.')[1]} · sin uso`
    else l = l.replace(/^\d+\.[A-Z] /, '')
  }
  if (v === '_/') v = 'sin señal'
  if (v === '--') v = 'sin sonda'
  return [l, v]
}

export const fmtClockSec = (t: string) =>
  /^\d{2}:\d{2}:\d{2}$/.test(t) ? t.slice(0, 5) : t

export const fmtIsoDate = (t: string) =>
  /^\d{4}-\d{2}-\d{2}$/.test(t)
    ? new Date(t + 'T12:00').toLocaleDateString('es-ES', {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
      })
    : t

export const MENU_HELP: Record<string, string> = {
  Idioma: 'Idioma de la pantalla del termostato',
  'Fecha & Hora': 'Reloj del equipo y cambio de hora',
  Ajustes: 'Dispositivos, habitaciones, zonas y WiFi',
  'Valores de servicio': 'Estado, avisos y comunicaciones',
  'Bloqueo de menú': 'Proteger los ajustes con contraseña',
  'Valores fábrica': 'Volver a la configuración original',
  'Gestión de dispositivos': 'Termostatos y centralitas emparejados',
  Habitaciones: 'Añadir o editar habitaciones',
  'Funciones °Cbox': 'Qué hace cada salida de la centralita',
  Zonas: 'Qué habitación controla cada zona',
  WiFi: 'Red, acceso y sensor WiFi',
  'Día V/I': 'Temperatura de cambio entre verano e invierno en modo normal',
  'Eco V/I': 'Temperatura de cambio entre verano e invierno en modo eco',
  'Modo de relé': 'Si el relé trabaja normalmente abierto o cerrado',
  'Protec. antibloqueo': 'Mueve la salida unos segundos si lleva mucho parada',
  'Retardo activ.': 'Espera antes de encender la bomba',
  'Retardo desactiv.': 'Tiempo que la bomba sigue tras apagarse',
  'Influencia habitación': 'Cuánto corrige la habitación la temperatura de impulsión',
  'Sensor de suelo': 'Sonda que limita la temperatura del suelo',
  'Sincron. habitación': 'Muestra aquí las habitaciones de otros °CALEON de la misma red, y las de este en ellos',
  'Modo de interfaz':
    'Menú completo o solo la temperatura. Para volver al completo, mantén pulsada la esquina superior derecha de la pantalla 5 segundos',
  Luminosidad: 'Brillo de la pantalla',
  Pantalla: 'Brillo de la pantalla',
  Histéresis: 'Margen alrededor de la temperatura objetivo antes de encender o apagar la zona',
  'Control de acceso': 'Correos que pueden conectarse desde la app, hasta cinco',
  'Sincronización horaria': 'Pone en hora el reloj desde la red; sin corriente el reloj aguanta 24 horas',
  'Hora de verano': 'Cambio automático entre horario de verano e invierno',
  'Compensación Sensor': 'Corrección manual de los sensores del termostato',
  'Termóstato 1': 'Conmuta una salida según la demanda de calor de las habitaciones elegidas',
  'Termostato 1': 'Conmuta una salida según la demanda de calor de las habitaciones elegidas',
  Deshumidificador: 'Enciende una salida cuando la humedad de la habitación supera el límite',
  'Cambio de estación': 'Cambia la bomba de calor entre calefacción y refrigeración cuando una habitación cambia de modo',
  'Horarios 1': 'Conmuta una salida según un horario propio',
  'Horarios 2': 'Conmuta una salida según un horario propio',
  Firmware: 'Versión del equipo y actualización',
  Mensajes: 'Registro de avisos y fallos del equipo',
  'Estado CAN': 'Estado del bus CAN entre termostatos y centralitas',
  'Añadir dispositivo': 'Busca equipos nuevos en el bus CAN',
  'Seleccionar red': 'Busca redes WiFi; el servidor no entra aquí al hacer copias',
}

/** tiles of room pickers only carry the room icon; show the room it stands for */
export function tileRoomName(icon: string | undefined, S: Snapshot | null): string {
  if (!ROOM_TYPES[icon || ''] && !/^room\d+$/.test(icon || '')) return ''
  return S?.rooms.find((r) => r.type === icon)?.name || typeLabel(icon)
}
