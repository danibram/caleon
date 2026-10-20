# Configuración y credenciales meteorológicas

La configuración meteorológica se guarda en `DATA_DIR/history.sqlite`, en la tabla `weather_config`. La selección conserva ubicación, proveedor, estación y fecha de inicio. La función apagada y `HISTORY=0` permiten guardar estos ajustes, pero impiden recoger temperaturas.

La tabla común `credentials` guarda credenciales con nombre, versión del cifrado, nonce y contenido cifrado. AEMET usa `weather.aemet.api_key`. Otros proveedores pueden usar el mismo almacén con otro nombre. La API devuelve únicamente si existe una clave configurada.

## Clave maestra

El servidor cifra con XChaCha20-Poly1305 y genera un nonce aleatorio de 24 bytes en cada guardado. El nombre de la credencial forma parte de los datos autenticados. Modificar el contenido o trasladarlo a otra credencial impide descifrarlo.

Al guardar la primera credencial, el servidor genera `DATA_DIR/secrets.key` con 32 bytes aleatorios y permisos `0600` en Unix. No genera una clave maestra si no hay credenciales. La creación instala el archivo completo sin sobrescribir una clave existente.

`CALEON_SECRETS_KEY_FILE` permite indicar otro archivo. Ese archivo debe existir y contener exactamente 32 bytes binarios. El servidor no lo genera ni lo modifica. Para trasladar una instalación existente, detén el servidor y mueve su `secrets.key` al destino indicado. Generar otra clave no permite descifrar las credenciales anteriores.

Una copia de SQLite contiene las credenciales cifradas. Quien tenga también la clave maestra podrá descifrarlas. Guarda una copia de esa clave por separado. Las copias de configuración del termostato excluyen estos datos.

Si falta la clave maestra, tiene otro contenido o hay credenciales dañadas, el servidor muestra un error y bloquea el guardado meteorológico. Conserva las filas existentes y no genera una clave sustitutiva. Restaura la clave original y reinicia el servidor.

## Docker

Para una instalación nueva, prepara un secreto fuera del directorio de datos.

```sh
mkdir -p secrets
chmod 700 secrets
openssl rand -out secrets/caleon.key 32
chmod 600 secrets/caleon.key
```

Añade el secreto al servicio de Compose. El resto de la configuración del servicio se mantiene.

```yaml
services:
  caleon:
    environment:
      CALEON_SECRETS_KEY_FILE: /run/secrets/caleon_master_key
    secrets:
      - caleon_master_key

secrets:
  caleon_master_key:
    file: ./secrets/caleon.key
```
