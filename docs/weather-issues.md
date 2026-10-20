# Integración de temperatura exterior

Implementación del diseño confirmado en [weather-integration-options.md](weather-integration-options.md).

La función empieza apagada. AEMET dispone de respaldo automático Open-Meteo, el panel exterior comparte el intervalo de las gráficas interiores y la recuperación cubre hasta 30 días donde existan temperaturas interiores.

La primera entrega establece tipos, configuración y persistencia. Las siguientes incorporan los adaptadores, la recogida, la configuración con mapa, las gráficas y la recuperación.

El seguimiento principal está en [#1](https://github.com/danibram/caleon/issues/1).

- [ ] [#2 Base de proveedores y persistencia](https://github.com/danibram/caleon/issues/2). Implementada y verificada localmente; pendiente de incorporar al repositorio remoto.
- [ ] [#3 Adaptador Open-Meteo y localidades](https://github.com/danibram/caleon/issues/3). Implementado y validado localmente con el proveedor real.
- [ ] [#4 Adaptador AEMET y estaciones](https://github.com/danibram/caleon/issues/4). Implementado sobre el contrato oficial; falta comprobar observaciones con una clave autorizada.
- [ ] [#5 Recogida, configuración y respaldo](https://github.com/danibram/caleon/issues/5). Implementado; respaldo, recuperación y apagado comprobados con proveedores controlados.
- [ ] [#6 Configuración con mapa](https://github.com/danibram/caleon/issues/6). Validada en escritorio y móvil con agent-browser; enlace de alta AEMET comprobado. La selección AEMET real requiere clave.
- [ ] [#7 Panel alineado y procedencia](https://github.com/danibram/caleon/issues/7). Validado localmente con datos reales Open-Meteo y temperaturas interiores de prueba.
- [ ] [#8 Importación y recuperación de huecos](https://github.com/danibram/caleon/issues/8). Implementadas y verificadas la recuperación manual y la ventana de 30 días.

Los issues siguen abiertos hasta incorporar los cambios al repositorio remoto. La [validación local](weather-local-validation.md) documenta el recorrido y sus límites.

## Primera entrega

El módulo `weather` define selecciones, fuentes y muestras, con una interfaz común para los adaptadores. La configuración se guarda aparte de la conexión del termostato y expone una representación pública sin la clave AEMET. Los ajustes se guardan en `weather_config` y las API keys cifradas en la tabla común `credentials` de SQLite. La clave maestra queda fuera de la base de datos, en un archivo con permisos privados o un secreto de Docker. Consulta [credenciales](weather-credentials.md).

SQLite conserva periodos de selección y procedencia. Las observaciones recuperadas tienen prioridad al consultar, sin borrar las estimaciones. Las consultas de cobertura interior permiten omitir importaciones sin temperaturas del termostato.

La base incluye 12 pruebas de integración y tipos TypeScript generados. Las entregas posteriores ya conectan adaptadores HTTP, recogida, rutas, configuración con mapa, panel y recuperación; el estado detallado figura arriba.
