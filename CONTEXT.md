# Contexto de CALEON

Este glosario recoge los términos acordados para comparar las medidas del termostato con la temperatura exterior.

## Temperatura exterior

**Ubicación de la vivienda**

Punto geográfico que representa la vivienda del termostato. Es la referencia elegida para las consultas de temperatura a Open-Meteo, también durante la sustitución de AEMET.

**Estación exterior**

Estación meteorológica física cuyas observaciones pueden usarse para comparar con las medidas del termostato. Su ubicación puede diferir de la ubicación de la vivienda.

**Fuente principal**

Selección del proveedor meteorológico y de la estación o el punto que aporta la temperatura exterior. Solo hay una fuente principal elegida para el termostato.

**Respaldo meteorológico**

Fuente que aporta temporalmente temperatura exterior cuando falla la principal. Open-Meteo es el respaldo de una estación AEMET y consulta la ubicación de la vivienda.

**Proveedor efectivo**

Proveedor que aporta la temperatura exterior durante un periodo determinado. Puede ser el proveedor principal o el respaldo meteorológico.

**Hueco meteorológico**

Periodo con medidas interiores que carece de la cobertura exterior esperada para la cadencia de su fuente. No representa una temperatura de cero grados.

**Recuperación de huecos**

Consulta de datos meteorológicos pasados para completar la comparación con las medidas interiores ya guardadas. Su alcance depende de la cobertura que conserve cada proveedor.

**Observación exterior**

Temperatura medida por una estación física, como la estación AEMET seleccionada para la vivienda.

**Estimación exterior**

Temperatura calculada por un modelo meteorológico para una ubicación, como la consulta Open-Meteo de la vivienda.
