# Conservar la procedencia al recuperar observaciones meteorológicas

AEMET puede fallar y Open-Meteo cubrir temporalmente el periodo con estimaciones para la vivienda. Cada muestra conserva su proveedor, estación o punto, producto y hora de validez; si después llegan observaciones AEMET del tramo sustituido, la gráfica les da prioridad y conserva el origen de las estimaciones guardadas.

Sobrescribir una única temperatura por hora perdería la distinción entre observación y estimación, que no podríamos reconstruir después. Conservar ambos orígenes añade almacenamiento y una regla de prioridad, pero permite explicar los cambios del histórico y los periodos de respaldo sin atribuir datos de modelo a una estación física.
