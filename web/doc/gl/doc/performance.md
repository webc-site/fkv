# Rendemento

`fkv_bench` Proba as cargas de escritura, lectura e exploración nun espazo de teclas fixo.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Os indicadores inclúen o rendemento, a latencia en cada etapa e o pico de RSS. lanzamento do fluxo de traballo x86_64 /
`fkv_bench` binario para aarch64.

## Encontro a tres bandas con RocksDB/fjall

`crate/perf/fkv_compare` Use o mesmo controlador de escenario e a mesma codificación de claves (subíndice big-endian de 8 bytes ao final,
A orde do dicionario é coherente coa orde dos subíndices) Proba fkv, RocksDB, fjall: RocksDB superado
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` A biblioteca compartida compilada cárgase con dlopen (o mesmo C
ABI, Rust build non se vincula a RocksDB), fjall usa `fjall 3` directamente. Cada un dos tres motores abre unha biblioteca.
Avance na mesma biblioteca na orde de fases: a carga masiva crea datos e, a continuación, len / lectura de puntos / lectura de intervalos /
Todas as eliminacións afectan aos mesmos datos.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Saltar a terceira columna se non hai RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Os números para a matriz da páxina de inicio só poden proceder dunha execución rastrexable:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "Execución de CI <id>"
```

`perf.yml` de GitHub Actions fai exactamente isto (instalar `librocksdb-dev`, programar adaptadores, executar
Matriz de filas de 25 M, carga artefacto e enche directamente a páxina cando `backfill=true`).

### calibre

- **Proporción de datos/caché**: 25.000.000 filas × 116 B ≈ 2766 MiB de datos, 256 MiB de caché por motor,
  ≈ 11:1, aliñado coa revisión oficial de RocksDB de 97 GiB de datos/6 GiB de caché (≈ 16:1).
- **A mesma cantidade de caché**: caché de páxinas de fkv (índice/datos mesmo grupo), caché de bloques de RocksDB
  (`cache_index_and_filter_blocks=true`, tamaño de bloque 4 KiB), caché de bloques propia de fjall
  Todos os orzamentos están configurados co mesmo número de bytes.
- **Calibre de persistencia (a columna de comparación usa a configuración predeterminada de RocksDB, sen aliñación)**: fkv 1 × páxina de datos + por lote
  1×manifest `fdatasync` (isomórfico a SyncData + AppendManifest de C++ EloqStore,
  Tampouco ten interruptor de apagado); a columna RocksDB usa a **configuración predeterminada** (WAL activado, `WriteOptions.sync=false`,
  Sen fsync secuencial), fjall tamén usa o valor predeterminado (`PersistMode::Buffer`). Polo tanto, a fase de escritura (escritura única /
  A diferenza entre as escrituras por lotes) é a **diferenza de forza de persistencia**, non a diferenza de implementación, que debe indicarse na páxina xunto co número.
  Se queres analizar "quen é máis rápido coa mesma forza de sincronización", engade `--align-fsync` (abre RocksDB con sync=true,
  fjall usa SyncAll), esta opción non se usa para matrices publicadas externamente.
- **Dous controladores fkv**: `--fkv-driver store` (predeterminado, conectado directamente a un único `Store` dentro do proceso e
  RocksDB / fjall tamén son chamadas de biblioteca) e `--fkv-driver runtime` (modo de implementación: un proceso
  `--fkv-shards` fíos de fragmentos, as solicitudes envíanse por clave, aliñadas coa lectura e escritura pública de C++ EloqStore
  interface). Cada solicitude deste último debe pasar pola cola + activación de fíos cruzados, nativa (macOS, o ficheiro IO pasa polo grupo de fíos)
  O tempo de lectura é duns 20 µs/tempo, o que é unha orde de magnitude máis lenta que a conexión directa en proceso; moito máis barato en Linux/io_uring, pero
  Aínda un salto. A matriz de páxina usa o calibre da tenda e o calibre do tempo de execución úsase para "formulario de implementación vs C++
  Comparación da "Interfaz pública" (a lectura dun fkv en tempo de execución de fkv é duns 50 kops/s, C++ EloqStore FFI dun fío único
  fragmento duns 21 kops/s).
- **Ler calibre de simultaneidade**: número de fío lateral fkv = `min(readers, --read-max-concurrency)`, por defecto
  O límite superior é 4 (alineado coa vCPU da máquina de avaliación), **non** "un fío por lector": o nivel superado só aumenta a cola;
  Caché por fío = 256 MiB/número de fíos (a cantidade total está fixada, non se tritura cos lectores) e úsase
  `FuturesUnordered` Mantén as solicitudes de `--read-window` (predeterminado 16) sobre a marcha. O lado de RocksDB é
  `readers` Cada `std::thread` le á vez; bucle dun só fío no lado do fjall (ignorando o equipamento dos lectores,
  Só como columna de referencia). `--no-fkv-read-threads` pode volver dexenerar a "N cadeas asíncronas intercaladas nun único tempo de execución".
  Esa é a profundidade do gasoduto, non a simultaneidade e non é comparable entre os motores.
- **len()**: ningún dos tres motores ten interfaces de reconto O(1). Esta liña é a taxa de reconto da táboa completa, non a busca O(1).