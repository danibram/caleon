export const MODES = [
  ['off', 'Apagado'],
  ['regular', 'Normal'],
  ['eco', 'Eco'],
  ['turbo', 'Confort'],
] as const

export type HeatMode = (typeof MODES)[number][0]

export const MODE_LABEL: Record<string, string> = {
  off: 'Apagado',
  regular: 'Normal',
  eco: 'Eco',
  turbo: 'Confort',
  on: 'Encendido',
  hws_on: 'ACS on',
  hws_off: 'ACS off',
}

export const DAYS = ['Lunes', 'Martes', 'Miércoles', 'Jueves', 'Viernes', 'Sábado', 'Domingo']

export const ROOM_TYPES: Record<string, string> = {
  living: 'Salón',
  dining: 'Comedor',
  kitchen: 'Cocina',
  bedroom1: 'Dormitorio 1',
  bedroom2: 'Dormitorio 2',
  bedroom3: 'Dormitorio 3',
  childrens1: 'Infantil 1',
  childrens2: 'Infantil 2',
  childrens3: 'Infantil 3',
  bathroom1: 'Baño 1',
  bathroom2: 'Baño 2',
  bathroom3: 'Baño 3',
  bathroom4: 'Baño 4',
  office1: 'Despacho 1',
  office2: 'Despacho 2',
  corridor1: 'Pasillo 1',
  corridor2: 'Pasillo 2',
  attic: 'Ático',
  cellar: 'Sótano',
  cellar2: 'Sótano 2',
  boiler_room: 'Sala de calderas',
  craft_room: 'Taller',
  food_storage: 'Despensa',
  laundry: 'Lavandería',
  libary: 'Biblioteca',
  locker: 'Trastero',
  pool: 'Piscina',
  walkin_wardrobe: 'Vestidor',
  wardrobe: 'Armario',
  floorfirst: 'Primera planta',
  floorground: 'Planta baja',
  outdoor: 'Exterior',
  home: 'Toda la casa',
}

export const SYSTEM = 65535
