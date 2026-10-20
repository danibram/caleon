# Diseño de integración de temperatura exterior

Diseño confirmado el 20 de octubre de 2026. Implementación en curso mediante los [issues de seguimiento](weather-issues.md).

## Activación y configuración

La función empieza apagada. Activarla abre la configuración, con Open-Meteo preseleccionado y un mapa para buscar una localidad o marcar la vivienda. Open-Meteo puede usarse sin clave. Elegir AEMET requiere introducir una clave válida y confirmar una estación.

El mapa preselecciona la estación más cercana con temperatura reciente. El usuario puede confirmar o elegir otra en el mapa o en una lista. Se muestran distancia, altitud y fecha de la última temperatura disponible. La cercanía no garantiza que una estación represente mejor el clima de la vivienda.

El campo de clave AEMET incluye "Obtener clave de AEMET", con enlace directo a https://opendata.aemet.es/centrodedescargas/altaUsuario en otra pestaña. La validación local del recorrido se hará con agent-browser, incluyendo el destino del enlace, la configuración, la cancelación y los estados de error.

Solo se guarda la activación cuando la configuración está completa. Cancelar una primera configuración deja la función apagada. Cambiar una configuración existente conserva la anterior hasta guardar la nueva.

La configuración es compartida por todos los navegadores conectados al servidor. Con la función activa y el histórico del servidor habilitado, se recogen datos aunque no haya navegadores abiertos y aparece un panel exterior alineado con las gráficas interiores. Apagar la función detiene la recogida y oculta el panel. Conserva ubicación, proveedor, estación, credenciales e histórico.

No hay otra casilla de visibilidad ni otra autorización para el respaldo Open-Meteo. El mapa solo carga al abrir su selector. Las consultas de búsqueda y validación se ejecutan al pedirlas durante la configuración.

## Fuente principal y respaldo

Hay una sola fuente principal elegida por el usuario. Con Open-Meteo se consulta la ubicación de la vivienda. Con AEMET se consulta la estación confirmada y Open-Meteo actúa como respaldo automático usando la ubicación de la vivienda. La interfaz explica ese respaldo al elegir AEMET.

La configuración conserva el proveedor principal y la estación aunque el proveedor efectivo cambie temporalmente. No se recogen dos series completas de forma permanente. Elegir Open-Meteo como principal no configura un respaldo AEMET implícito.

La política inicial de sustitución acordada es la siguiente.

- Un fallo de conexión provoca un reintento. Si también falla, se usa Open-Meteo.
- Una clave rechazada durante la configuración impide guardar AEMET como principal. Si una clave que funcionaba deja de funcionar, se activa el respaldo y se muestra un aviso para corregirla.
- Una respuesta correcta cuya última temperatura tenga más de dos horas activa el respaldo.
- Mientras se usa el respaldo, se comprueba AEMET periódicamente. Se vuelve cuando aporte una temperatura reciente y válida, no solo una respuesta HTTP correcta.
- Una respuesta vacía para un periodo histórico no activa la sustitución de proveedor.
- Si ambos proveedores fallan, se conservan los datos anteriores y los huecos. Se muestra la antigüedad de la última lectura.

La política se implementa internamente sin controles de tiempos o reintentos en Ajustes. El umbral inicial de dos horas se validará con datos reales de AEMET antes de cerrar la implementación. Una credencial rechazada requiere comprobaciones de recuperación espaciadas, sin repetir peticiones de autenticación en cada refresco de la gráfica.

## Histórico, procedencia y recuperación

Cada muestra conserva proveedor, estación o punto, producto, tipo de dato, hora de validez UTC, resolución y hora de descarga. Se distinguen observaciones y estimaciones. Los cambios de proveedor efectivo son visibles en la gráfica, con el estado "Open-Meteo temporal · AEMET no disponible" durante una sustitución.

Las estimaciones no se guardan como observaciones AEMET. Si AEMET recupera observaciones del periodo sustituido, se muestran con prioridad sobre las estimaciones sin borrar la procedencia de estas últimas. La recogida intenta recuperar las observaciones disponibles de ese periodo. La razón de conservar ambos orígenes se recoge en [ADR 0001](adr/0001-weather-sample-provenance.md).

La importación inicial recupera hasta 30 días del proveedor principal, limitada a los periodos que tengan temperaturas interiores guardadas. Si no hay medidas interiores, no se solicita histórico exterior. AEMET solo aporta la ventana que permita. El respaldo no completa otras semanas anteriores con estimaciones.

Cambiar estación o proveedor para la misma vivienda conserva el histórico visible y marca el cambio de fuente. La nueva selección no reescribe el origen de las muestras anteriores. Cada selección tiene una identidad y un periodo de aplicación para consultar la fuente correspondiente a cada tramo.

Al reactivar se intenta recuperar los huecos que permita el proveedor. El botón "Recuperar huecos" intenta siempre completar los últimos 30 días, independientemente del rango mostrado. Actúa solo sobre periodos con temperaturas interiores y consulta la fuente correspondiente a cada tramo sin cambiar el proveedor principal. También puede recuperar observaciones AEMET de tramos cubiertos temporalmente con estimaciones.

El botón queda deshabilitado con la función apagada o con `HISTORY=0`. Al terminar muestra los huecos completados y los que siguen sin datos disponibles. Las consultas se agrupan por fuente y periodo para evitar una petición por cada muestra ausente. La detección de huecos respeta la cadencia de cada fuente; una serie horaria no necesita una temperatura exterior por cada minuto interior.

Una respuesta vacía, un error temporal y un periodo sin cobertura son resultados distintos. Ninguno equivale a una temperatura de cero grados. No se garantiza recuperar datos que el proveedor ya no conserve.

## Gráfica

La primera versión muestra un panel exterior debajo de la gráfica interior, con los mismos extremos temporales. No incluye superposición ni selector entre vistas. Conserva la atribución del proveedor y distingue observación y estimación, incluso en tramos históricos.

El inicio de cada panel no se recorta de forma independiente. El umbral para unir puntos y buscar valores con el cursor depende de la cadencia de cada serie. No se fabrican lecturas por minuto a partir de temperaturas horarias. Los huecos superiores a la cadencia admitida quedan visibles.

## Diseño técnico propuesto

Un módulo `weather` en el servidor Rust contiene adaptadores AEMET y Open-Meteo detrás de una interfaz común. Esa interfaz expresa capacidades, resuelve una selección y obtiene temperaturas para un periodo. La búsqueda de estaciones es opcional para cada proveedor. Añadir un proveedor exige implementar su adaptador y registrarlo, sin modificar las gráficas ni inventar estaciones para proveedores por coordenadas.

El módulo concentra autenticación, normalización, caché, límites, reintentos, sustitución y recuperación. Una tarea independiente recoge datos sin bloquear el sondeo del termostato. El navegador recibe configuración sin secretos, candidatos, estado de recogida y series normalizadas. Las peticiones meteorológicas salen del servidor y la clave AEMET no se devuelve ni se registra en URLs de logs.

La configuración meteorológica se guarda en `weather_config` y las API keys cifradas en la tabla común `credentials` de `history.sqlite`. La clave maestra queda fuera de SQLite, en un archivo privado o un secreto de Docker. Los detalles están en [credenciales](weather-credentials.md). La configuración mantiene rutas propias bajo `/api/weather`, separadas de `connection::Settings`. El guardado actual de conexión busca y prueba el termostato y no se reutiliza para cambiar una estación. Las muestras y la cobertura se guardan en tablas propias de la SQLite existente, con deduplicación por identidad de la serie y hora de validez.

Se propone Leaflet con carga diferida y una capa de teselas sustituible. El buscador de localidades es independiente del proveedor meteorológico; Open-Meteo Geocoding es la opción inicial. El mapa muestra estaciones físicas solo para AEMET y un punto para Open-Meteo. Se conservan tanto las coordenadas solicitadas como las que devuelva el proveedor.

Las teselas OSM requieren atribución visible y respetar su caché. Su servidor público no garantiza disponibilidad, por lo que la capa debe poder cambiarse. Referencias en [Leaflet](https://leafletjs.com/examples/quick-start/), [Geocoding](https://open-meteo.com/en/docs/geocoding-api) y [política de teselas](https://operations.osmfoundation.org/policies/tiles/).

## Modos del servidor y copias

| Condición | Comportamiento meteorológico acordado |
| --- | --- |
| Función apagada | No recoge ni recupera datos; oculta el panel y conserva la configuración y el histórico. |
| `HISTORY=0` | Impide la recogida y la recuperación meteorológicas; la interfaz explica el motivo. |
| `READ_ONLY=1` | Permite configurar meteorología porque no modifica el termostato. |
| `WEB=0` | Mantiene API y tareas de fondo, siguiendo el modelo actual. |
| Copia del equipo | Excluye ubicación, credenciales e histórico meteorológico. |

Copiar los datos del servidor o su SQLite es una operación independiente de las copias del equipo. No se amplía el sistema de copias en este trabajo. Tampoco se corrigen otros comportamientos existentes, como el refresco manual que guarda muestras interiores con `HISTORY=0`.

## Comprobaciones para la implementación

La entrevista no deja decisiones de producto pendientes. Antes de cerrar el adaptador AEMET se deben comprobar con una clave autorizada el catálogo de estaciones, la presencia de temperatura reciente, el formato de coordenadas y fechas, la ventana disponible y el umbral de antigüedad. No se han usado credenciales ni probado endpoints autenticados en este estudio.

La validación debe cubrir función apagada, `HISTORY=0`, ausencia de temperaturas interiores, sustitución y recuperación de AEMET, prioridad de observaciones tardías, procedencia tras cambiar fuente, recuperación limitada a 30 días y comparación temporal con distintas cadencias. Los errores meteorológicos no deben interrumpir el sondeo del termostato.

El vocabulario está en [el glosario](../CONTEXT.md) y las capacidades publicadas en [la investigación de proveedores](weather-providers-research.md).
