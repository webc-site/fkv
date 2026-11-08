# Performance

`fkv_bench` Sulayi ang pagsulat, pagbasa, ug pag-scan sa mga load sa usa ka fixed key space.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Ang mga timailhan naglakip sa throughput, latency sa matag yugto, ug peak RSS. buhian workflow release x86_64 /
`fkv_bench` binary para sa aarch64.

## Tulo ka paagi nga matchup sa RocksDB/fjall

`crate/perf/fkv_compare` Gamita ang parehas nga stage driver ug parehas nga key encoding (8-byte big-endian subscript sa katapusan,
Ang han-ay sa diksyonaryo nahiuyon sa han-ay sa subscript) Pagsulay sa fkv, RocksDB, fjall: Nakapasar ang RocksDB
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Ang gihugpong nga shared library kay puno sa dlopen (parehas nga C
ABI, Rust build wala mag-link sa RocksDB), fjall naggamit `fjall 3` direkta. Ang matag usa sa tulo ka mga makina nagbukas sa usa ka librarya.
Pag-uswag sa parehas nga librarya sa han-ay sa mga yugto: ang kadaghanan nga load nagmugna og datos, ug dayon len / point reading / range reading /
Ang tanan nga mga pagtangtang makaapekto sa parehas nga datos.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Laktawan ang ikatulo nga column kung walay RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Ang mga numero alang sa homepage matrix mahimo ra gikan sa usa ka masubay nga run:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

Ang GitHub Actions' `perf.yml` nagbuhat niini sa eksakto (i-install `librocksdb-dev`, mga adaptor sa programa, pagdagan
25M row matrix, i-upload ang artifact, ug direkta nga i-backfill ang panid kung `backfill=true`).

### kalibre

- **Data/Cache Ratio**: 25,000,000 row × 116 B ≈ 2766 MiB sa data, 256 MiB sa cache kada makina,
  ≈ 11:1, nahiuyon sa opisyal nga pagrepaso sa RocksDB sa 97 GiB data/6 GiB cache (≈ 16:1).
- **Parehas nga gidaghanon sa cache**: fkv's page cache (index/data same pool), block cache sa RocksDB
  (`cache_index_and_filter_blocks=true`, block size 4 KiB), kaugalingong block cache ni fjall
  Ang tanan nga mga badyet gitakda sa parehas nga gidaghanon sa mga byte.
- **Persistence caliber (ang kolum sa pagtandi naggamit sa RocksDB default configuration, walay alignment)**: fkv 1×data page + kada batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest sa C++ EloqStore,
  Ni adunay off switch); ang kolum sa RocksDB naggamit sa **default configuration** (WAL on, `WriteOptions.sync=false`,
  Walay sequential fsync), fjall usab naggamit sa default (`PersistMode::Buffer`). Busa ang yugto sa pagsulat (usa ka pagsulat /
  Ang kalainan tali sa batch writes) mao ang **persistence strength difference**, dili ang implementasyon nga kalainan, nga kinahanglang ipahayag sa panid uban sa numero.
  Kung gusto nimong analisahon "kinsa ang mas paspas sa ilawom sa parehas nga kusog sa pag-synchronize", idugang `--align-fsync` (abli ang RocksDB nga adunay sync = true,
  fjall naggamit sa SyncAll), kini nga opsyon wala gigamit alang sa externally published matrices.
- **Duha ka fkv driver**: `--fkv-driver store` (default, direktang konektado sa usa ka `Store` sulod sa proseso, ug
  Ang RocksDB / fjall kay mga tawag sa library) ug `--fkv-driver runtime` (deployment mode: usa ka proseso
  `--fkv-shards` mga shard thread, ang mga hangyo gi-ruta pinaagi sa yawe, gipahiangay sa publiko nga pagbasa ug pagsulat sa C++ EloqStore
  interface). Ang matag hangyo sa naulahi kinahanglan nga moagi sa pila + cross-thread wake-up, lumad (macOS, file IO moagi sa thread pool)
  Ang oras sa pagbasa mga 20 µs/oras, nga usa ka han-ay sa magnitude nga mas hinay kaysa sa proseso nga direktang koneksyon; mas barato sa Linux/io_uring, pero
  Ambak gihapon. Ang page matrix naggamit sa store caliber, ug ang runtime caliber gigamit para sa "deployment form vs C++
  Pagtandi sa "Public Interface" (fkv runtime nga single-thread nga pagbasa mga 50 kops/s, C++ EloqStore FFI single-thread
  shard mga 21 kops/s).
- **Basaha ang concurrency caliber**: fkv side thread number = `min(readers, --read-max-concurrency)`, default
  Ang taas nga limitasyon mao ang 4 (nahiangay sa evaluation machine vCPU), **dili** "usa ka thread kada magbabasa" - ang milapas nga lebel nagdugang lamang sa pila;
  Cache matag hilo = 256 MiB / gidaghanon sa mga hilo (ang kinatibuk-ang kantidad gitakda, dili giputol sa mga magbabasa), ug gigamit
  `FuturesUnordered` Ipadayon ang `--read-window` (default 16) nga mga hangyo diha-diha dayon. Ang bahin sa RocksDB mao ang
  `readers` Matag `std::thread` dungan nga basahon; single-threaded loop sa fjall side (wala magtagad sa gamit sa mga magbabasa,
  Isip usa ka reference column lamang). `--no-fkv-read-threads` mahimong madaot balik sa "interleaved N asynchronous chains sa usa ka runtime".
  Kana ang giladmon sa pipeline, dili concurrency, ug dili ikatandi sa mga makina.
- **len()**: Walay usa sa tulo ka makina ang adunay O(1) nga mga interface sa pag-ihap. Kini nga linya mao ang tibuuk nga rate sa pag-ihap sa lamesa, dili O(1) nga pagpangita.