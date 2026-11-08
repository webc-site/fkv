# Rendimiento

`fkv_bench` Pruebe cargas de escritura, lectura y escaneo en un espacio de clave fijo.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Los indicadores incluyen rendimiento, latencia en cada etapa y RSS máximo. lanzamiento de flujo de trabajo x86_64 /
`fkv_bench` binario para aarch64.

## Enfrentamiento a tres bandas con RocksDB/fjall

`crate/perf/fkv_compare` Utilice el mismo controlador de etapa y la misma codificación de clave (subíndice big-endian de 8 bytes al final,
El orden del diccionario es consistente con el orden de los subíndices) Prueba fkv, RocksDB, fjall: RocksDB pasó
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` La biblioteca compartida compilada se carga con dlopen (el mismo C
ABI, la compilación de Rust no se vincula a RocksDB), fjall usa `fjall 3` directamente. Cada uno de los tres motores abre una biblioteca.
Avance en la misma biblioteca en el orden de las etapas: la carga masiva crea datos y luego len/lectura de puntos/lectura de rango/
Todas las eliminaciones afectan a los mismos datos.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Omita la tercera columna si no hay RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Los números de la matriz de la página de inicio solo pueden provenir de una ejecución rastreable:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "Ejecución de CI <id>"
```

`perf.yml` de GitHub Actions hace exactamente esto (instalar `librocksdb-dev`, programar adaptadores, ejecutar
Matriz de filas de 25 millones, cargar artefactos y rellenar directamente la página cuando `backfill=true`).

### calibre

- **Relación datos/caché**: 25 000 000 filas × 116 B ≈ 2766 MiB de datos, 256 MiB de caché por motor,
  ≈ 11:1, alineado con la revisión oficial de RocksDB de datos de 97 GiB/caché de 6 GiB (≈ 16:1).
- **Misma cantidad de caché**: caché de página de fkv (índice/datos mismo grupo), caché de bloque de RocksDB
  (`cache_index_and_filter_blocks=true`, tamaño de bloque 4 KiB), caché de bloque propio de fjall
  Todos los presupuestos están configurados con el mismo número de bytes.
- **Calibre de persistencia (la columna de comparación usa la configuración predeterminada de RocksDB, sin alineación)**: fkv 1×página de datos + por lote
  1×manifest `fdatasync` (Isomórfico a SyncData + AppendManifest de C++ EloqStore,
  Ninguno tiene interruptor de apagado); la columna RocksDB usa la **configuración predeterminada** (WAL activado, `WriteOptions.sync=false`,
  Sin fsync secuencial), fjall también usa el valor predeterminado (`PersistMode::Buffer`). Por lo tanto, la fase de escritura (escritura única /
  La diferencia entre escrituras por lotes es la **diferencia de fuerza de persistencia**, no la diferencia de implementación, que debe indicarse en la página junto con el número.
  Si desea analizar "quién es más rápido con la misma potencia de sincronización", agregue `--align-fsync` (abra RocksDB con sync=true,
  fjall usa SyncAll), esta opción no se usa para matrices publicadas externamente.
- **Dos controladores fkv**: `--fkv-driver store` (predeterminado, conectado directamente a un único `Store` dentro del proceso, y
  RocksDB/fjall también son llamadas a biblioteca) y `--fkv-driver runtime` (modo de implementación: un proceso
  `--fkv-shards` subprocesos de fragmentos, las solicitudes se enrutan por clave, alineadas con la lectura y escritura públicas de C++ EloqStore
  interfaz). Cada solicitud de este último debe pasar por la cola + activación entre subprocesos, nativa (macOS, el archivo IO pasa por el grupo de subprocesos)
  El tiempo de lectura es de aproximadamente 20 µs/tiempo, que es un orden de magnitud más lento que la conexión directa en proceso; mucho más barato en Linux/io_uring, pero
  Todavía un salto. La matriz de página usa el calibre de la tienda y el calibre del tiempo de ejecución se usa para "formulario de implementación versus C++
  Comparación de la "Interfaz pública" (la lectura de un solo subproceso en tiempo de ejecución de fkv es de aproximadamente 50 kops/s, C++ EloqStore FFI de un solo subproceso)
  fragmento aproximadamente 21 kops/s).
- **Leer calibre de concurrencia**: número de hilo lateral fkv = `min(readers, --read-max-concurrency)`, predeterminado
  El límite superior es 4 (alineado con la vCPU de la máquina de evaluación), **no** "un subproceso por lector"; el nivel excedido solo aumenta la cola;
  Caché por subproceso = 256 MiB / número de subprocesos (la cantidad total es fija, no se destruye con lectores) y se usa
  `FuturesUnordered` Mantenga `--read-window` (predeterminado 16) solicitudes sobre la marcha. El lado de RocksDB es
  `readers` Cada `std::thread` lee simultáneamente; bucle de un solo hilo en el lado fjall (ignorando el equipo de los lectores,
  Sólo como columna de referencia). `--no-fkv-read-threads` puede volver a degenerar en "N cadenas asíncronas entrelazadas en un único tiempo de ejecución".
  Eso es profundidad de canalización, no simultaneidad, y no es comparable entre motores.
- **len()**: Ninguno de los tres motores tiene interfaces de conteo O(1). Esta línea es la tasa de recuento de la tabla completa, no la búsqueda O(1).