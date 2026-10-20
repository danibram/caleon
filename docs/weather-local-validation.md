# Validación local de temperatura exterior

La comprobación del 20 de octubre de 2026 usa la interfaz local con agent-browser, la API meteorológica del servidor y Open-Meteo real. Las temperaturas interiores proceden de fixtures. El proceso no descubre ni conecta un termostato y rechaza cualquier intento de invocarlo.

## Recorrido comprobado

- La función aparece apagada inicialmente. Cancelar la configuración conserva ese estado.
- Open-Meteo está preseleccionado y permite buscar Madrid, seleccionar la vivienda en el mapa y guardar sin clave.
- AEMET muestra un campo de clave y el enlace "Obtener clave de AEMET". El enlace abrió otra pestaña en el formulario oficial "Obtención API Key" de [AEMET OpenData](https://opendata.aemet.es/centrodedescargas/altaUsuario). No se envió el formulario de alta.
- Sin clave y estación, no se puede guardar AEMET. La clave guardada nunca se devuelve al navegador.
- La activación Open-Meteo obtiene temperaturas reales, muestra el panel exterior y recupera la cobertura interior disponible. La primera recuperación completó 13 horas y no dejó horas pendientes en la muestra.
- "Recuperar huecos de los últimos 30 días" ejecuta otra consulta y muestra el resultado sin duplicar datos.
- Reiniciar el servidor conserva activación, ubicación e histórico.
- Apagar detiene la recogida y oculta el panel, conservando configuración e histórico.
- El recorrido se ha comprobado en escritorio y con viewport de 390 por 844. La página no presenta desbordamiento horizontal.
- Se corrigió el redibujado al cambiar el ancho para evitar que las gráficas quedasen reducidas a una escala de escritorio en móvil.

## Pruebas automáticas

Las pruebas de persistencia y configuración cubren inicialización de tablas SQLite, aislamiento por estación y vivienda, deduplicación, procedencia, prioridad AEMET, cobertura interior, cifrado y permisos de la clave maestra. También comprueban que una clave perdida, sustituida o dañada impide sobrescribir las credenciales, y que un fallo de guardado revierte la escritura de configuración y credenciales. El almacén común tiene pruebas de nombres independientes, nonces nuevos, rechazo de credenciales trasladadas a otro nombre y claves externas. Otras pruebas comprueban la normalización horaria Open-Meteo, las fechas UTC AEMET y ocho pruebas de recogida.

Los escenarios de recogida usan proveedores controlados para comprobar función apagada, `HISTORY=0`, reintento, respaldo, clave rechazada, observaciones antiguas, vuelta a AEMET, apagado durante una petición, fallo de ambos proveedores conservando la antigüedad del último dato y recuperación limitada a 30 días. El transporte del termostato falla la prueba si se invoca.

Han pasado las pruebas del workspace, Clippy con advertencias de código tratadas como errores, Oxlint y la compilación TypeScript/Vite. Permanece la advertencia previa de ts-rs sobre un atributo serde de Room.

## Límite de la validación AEMET

Se han comprobado la especificación, los metadatos públicos, las fechas y la página de alta oficiales. No se dispone de una clave autorizada para comprobar un payload real de observaciones, el catálogo operativo y el respaldo ante un fallo real de AEMET. Ese requisito sigue pendiente en el issue #4. Las pruebas de respaldo usan respuestas controladas y no sustituyen esa comprobación.

## Reproducir

Arrancar la API de validación en una terminal.

```sh
DATA_DIR=/tmp/caleon-weather-local cargo run -p caleon-server --example weather_local --locked
```

Arrancar la interfaz en otra terminal desde `web/`.

```sh
VITE_MOCK=1 CALEON_WEATHER_API=http://127.0.0.1:18089 pnpm dev --host 127.0.0.1 --port 5177
```

Abrir [Ajustes en local](http://127.0.0.1:5177/#/settings). El ejemplo guarda únicamente sus datos de prueba en el directorio indicado. El mapa carga al abrir la configuración y las peticiones meteorológicas salen de la API Rust.

## Capturas

[Panel exterior en móvil](screenshots/weather-charts-mobile.png) y [formulario AEMET en móvil](screenshots/weather-settings-mobile.png). Las capturas usan medidas interiores de prueba y no incluyen claves.
