# Proveedores de temperatura exterior

Investigación del 20 de octubre de 2026. Este documento recoge capacidades publicadas y decisiones propuestas. No se han probado credenciales ni respuestas autenticadas de AEMET.

## Diferencia entre proveedores

AEMET aporta observaciones de estaciones físicas. Open-Meteo calcula series para coordenadas mediante modelos meteorológicos. El mapa debe mostrar estaciones seleccionables para AEMET y un punto de consulta para Open-Meteo. Presentar ambos como estaciones ocultaría una diferencia que afecta a la interpretación de las curvas. [AEMET OpenData](https://opendata.aemet.es/dist/), [Open-Meteo Forecast](https://open-meteo.com/en/docs).

| Capacidad | AEMET | Open-Meteo |
| --- | --- | --- |
| Selección | Estación identificada por `idema` | Latitud y longitud |
| Tipo de temperatura | Observación de estación | Estimación de modelo |
| Credencial | API key | Sin clave en acceso gratuito no comercial |
| Histórico horario al configurar | Ventana reciente limitada | Disponible mediante días pasados o archivo |
| Acumulación local | Necesaria para conservar semanas y meses horarios | Conveniente para conservar procedencia y servir sin conexión |

## AEMET

La documentación enumera inventarios climatológicos y los recursos `/api/observacion/convencional/todas` y `/api/observacion/convencional/datos/estacion/{idema}`. Los recursos históricos climatológicos documentados son diarios y mensuales/anuales. No se ha encontrado un recurso general que garantice recuperar 30 días horarios de cualquier estación. [Documentación oficial](https://opendata.aemet.es/dist/).

La especificación oficial OpenAPI 3.0.1, versión 2.0, describe tanto `/api/observacion/convencional/todas` como `/api/observacion/convencional/datos/estacion/{idema}` con observaciones horarias de las últimas 12 horas y actualización continua. Esto confirma el alcance documentado del producto; no garantiza cobertura completa de todas las estaciones ni acceso a días anteriores. [Especificación descargable](https://opendata.aemet.es/AEMET_OpenData_specification.json).

El inventario climatológico no debe tratarse como lista de estaciones operativas que publican temperatura reciente. Antes de ofrecer una estación, se propone contrastar el inventario con las estaciones presentes en observaciones recientes y comprobar que existe temperatura válida. Las estaciones sin datos recientes pueden aparecer diferenciadas, sin prometer que servirán para la comparación.

Los metadatos oficiales de observación se han podido consultar sin clave. Declaran `idema` como texto, `lat` y `lon` como grados decimales, `alt` en metros y `ubi` como nombre de estación. Todos esos campos son requeridos. `fint` y `ta` son opcionales. `fint` representa la fecha final del periodo en UTC y su formato documentado es `AAAA-MM-DDTHH:MM:SS`; `ta` es la temperatura instantánea del aire en grados Celsius correspondiente a esa fecha. Para temperatura no se resta una hora al sello `fint`. [Metadatos oficiales consultados](https://opendata.aemet.es/opendata/sh/55c2971b).

De esos metadatos se deduce que el recurso de observaciones de todas las estaciones puede proporcionar por sí mismo candidatos operativos con coordenadas y altitud, filtrando registros con fecha y temperatura válidas y conservando el más reciente por `idema`. Es una vía de implementación, pendiente de probar con un payload autenticado. El inventario climatológico no es necesario para esa selección, aunque puede ampliar metadatos. No se debe fijar una cadencia diezminutal ni una latencia máxima con esta documentación.

La descarga pública de esos metadatos devolvió el 20 de octubre de 2026 `Content-Type: text/plain;charset=ISO-8859-15`. La decodificación UTF-8 directa falló por los acentos. El cliente debe respetar la codificación de cada respuesta antes de deserializar JSON; esta observación no demuestra que todos los recursos usen la misma codificación.

La API requiere una clave en la cabecera `api_key`, según `components.securitySchemes` de la especificación. La primera respuesta tiene `estado`, `descripcion`, `datos` y `metadatos`; estos dos últimos solo están tipados como texto. El contrato no garantiza en su esquema un host ni HTTPS. El adaptador debe validar los enlaces recibidos y evitar reenviar la clave al salto de descarga. [Especificación oficial](https://opendata.aemet.es/AEMET_OpenData_specification.json).

Los enlaces de datos y metadatos son temporales, activos unos cinco minutos. El adaptador debe descargar el contenido y guardar datos normalizados, nunca esos enlaces. Las FAQ publican un límite de 40 conexiones por minuto y usuario, junto con un límite global. [FAQ oficiales, apartados 1.9, 2.1 y 3.5](https://opendata.aemet.es/centrodedescargas/docs/FAQs220424.pdf).

El enlace oficial para crear una clave es [Obtención API Key](https://opendata.aemet.es/centrodedescargas/altaUsuario). La página muestra un campo de correo, comprobación y botón de envío. La interfaz de configuración debe enlazar directamente esa página, sin solicitar ni automatizar el alta en nombre del usuario.

La atribución y las condiciones aplicables deben conservarse con la serie y mostrarse junto a la gráfica. La referencia normativa publicada por AEMET es su [nota legal](https://www.aemet.es/es/nota_legal).

## Open-Meteo

`/v1/forecast` acepta coordenadas, `hourly=temperature_2m`, `past_days` entre 0 y 92, y `timeformat=unixtime`. Esto permite consultar los rangos actuales de 6 horas, 24 horas, 7 días y 30 días sin esperar a acumularlos. La temperatura corresponde al aire a dos metros. Las condiciones actuales también proceden de modelos. La frecuencia de actualización depende del modelo, por lo que una resolución temporal de 15 minutos no significa una observación nueva cada 15 minutos. [Parámetros y fuentes](https://open-meteo.com/en/docs).

El archivo histórico ofrece series horarias de reanálisis y modelos. ERA5 y ERA5-Land llegan con unos cinco días de retraso; el producto IFS tiene otra disponibilidad. No conviene completar una misma curva con productos distintos sin registrar el origen de cada tramo. [Historical Weather API](https://open-meteo.com/en/docs/historical-weather-api). El archivo de pronósticos pasados ofrece otra vía para mantener una serie de modelos comparable con Forecast. [Historical Forecast API](https://open-meteo.com/en/docs/historical-forecast-api).

El acceso gratuito permite domótica personal y otros usos no comerciales, con menos de 10.000 llamadas al día, 5.000 por hora y 600 por minuto. El uso comercial requiere otro acceso. [Condiciones](https://open-meteo.com/en/terms). Los datos usan CC BY 4.0 y requieren atribución, enlace a licencia e indicación de modificaciones. [Licencia](https://open-meteo.com/en/licence).

## Cobertura de las gráficas

| Rango | AEMET al configurar | AEMET tras acumular | Open-Meteo |
| --- | --- | --- | --- |
| 6 horas | Solicitar ventana reciente; admitir huecos | Datos locales y actualización | Serie horaria pasada |
| 24 horas | Puede quedar incompleto | Completo si hubo cobertura y recogida | Serie horaria pasada |
| 7 días | Sin garantía de recuperar observaciones horarias | Disponible tras acumular | Serie horaria pasada |
| 30 días | Sin garantía de recuperar observaciones horarias | Disponible tras acumular | Serie horaria pasada |

## Recomendaciones de implementación

- Mantener adaptadores separados y declarar capacidades de selección e histórico. Un proveedor futuro no debería verse obligado a inventar estaciones.
- Guardar observación o estimación, proveedor, estación o punto, producto, hora del dato, hora de descarga, unidad y atribución.
- Persistir en UTC y convertir a la zona elegida al presentar. Evitar comparar horas locales ambiguas en cambios de horario.
- Consultar desde el servidor, con límites y reintentos. Como propuesta inicial, revisar AEMET cada 15 minutos y Open-Meteo cada hora, ajustando tras comprobar sus datos. Estas cadencias son decisiones del proyecto.
- Separar las series al cambiar estación, coordenadas o producto. Conservar los datos previos sin unirlos a otra ubicación.
- Mantener huecos cuando falten medidas. No rellenar observaciones de AEMET con estimaciones de Open-Meteo bajo el mismo nombre.
- Mostrar antigüedad del último dato y cobertura disponible. La distancia y diferencia de altitud ayudan a elegir estación, pero no garantizan que represente la vivienda.
