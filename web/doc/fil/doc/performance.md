# Pagganap

`fkv_bench` Subukang magsulat, magbasa, at mag-scan ng mga load sa isang nakapirming key space.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Kasama sa mga tagapagpahiwatig ang throughput, latency sa bawat yugto, at peak RSS. release workflow release x86_64 /
`fkv_bench` binary para sa aarch64.

## Three-way matchup sa RocksDB/fjall

`crate/perf/fkv_compare` Gamitin ang parehong stage driver at ang parehong key encoding (8-byte big-endian subscript sa dulo,
Ang pagkakasunud-sunod ng diksyunaryo ay pare-pareho sa pagkakasunud-sunod ng subscript) Pagsubok sa fkv, RocksDB, fjall: Naipasa ang RocksDB
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Ang pinagsama-samang shared library ay ni-load ng dlopen (kaparehong C
ABI, hindi nagli-link ang Rust build sa RocksDB), direktang gumagamit si fjall ng `fjall 3`. Ang bawat isa sa tatlong makina ay nagbubukas ng isang silid-aklatan.
Mag-advance sa parehong library sa pagkakasunud-sunod ng mga yugto: ang bulk load ay lumilikha ng data, at pagkatapos ay len / point reading / range reading /
Ang lahat ng mga pagtanggal ay nakakaapekto sa parehong data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Laktawan ang ikatlong column kung walang RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Ang mga numero para sa homepage matrix ay maaari lamang magmula sa isang traceable run:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

Eksaktong ginagawa ito ng `perf.yml` ng GitHub Actions (i-install ang `librocksdb-dev`, mga adaptor ng program, patakbuhin
25M row matrix, mag-upload ng artifact, at direktang i-backfill ang page kapag `backfill=true`).

### kalibre

- **Data/Cache Ratio**: 25,000,000 row × 116 B ≈ 2766 MiB ng data, 256 MiB ng cache bawat engine,
  ≈ 11:1, nakahanay sa opisyal na pagsusuri ng RocksDB sa 97 GiB data/6 GiB cache (≈ 16:1).
- **Parehong dami ng cache**: page cache ng fkv (index/data same pool), block cache ng RocksDB
  (`cache_index_and_filter_blocks=true`, block size 4 KiB), sariling block cache ni fjall
  Ang mga badyet ay nakatakda sa parehong bilang ng mga byte.
- **Persistence caliber (ginagamit ng column ng paghahambing ang default na configuration ng RocksDB, nang walang alignment)**: fkv 1×data page + per batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest ng C++ EloqStore,
  Wala ring naka-off na switch); ginagamit ng column ng RocksDB ang **default na configuration** (WAL on, `WriteOptions.sync=false`,
  Walang sequential fsync), ginagamit din ng fjall ang default (`PersistMode::Buffer`). Samakatuwid ang yugto ng pagsulat (iisang pagsulat /
  Ang pagkakaiba sa pagitan ng batch writes) ay ang **persistence strength difference**, hindi ang implementation difference, na dapat na nakasaad sa page kasama ang numero.
  Kung gusto mong suriin ang "sino ang mas mabilis sa ilalim ng parehong lakas ng pag-synchronize", idagdag ang `--align-fsync` (buksan ang RocksDB na may sync=true,
  fjall ay gumagamit ng SyncAll), ang opsyong ito ay hindi ginagamit para sa mga external na na-publish na matrice.
- **Dalawang fkv driver**: `--fkv-driver store` (default, direktang konektado sa isang `Store` sa loob ng proseso, at
  Ang RocksDB / fjall ay mga tawag din sa library) at `--fkv-driver runtime` (deployment mode: isang proseso
  `--fkv-shards` shard thread, ang mga kahilingan ay niruruta sa pamamagitan ng key, na nakahanay sa pampublikong pagbabasa at pagsulat ng C++ EloqStore
  interface). Ang bawat kahilingan ng huli ay kailangang dumaan sa queue + cross-thread wake-up, native (macOS, file IO ay dumaan sa thread pool)
  Ang oras ng pagbabasa ay humigit-kumulang 20 µs/oras, na isang pagkakasunud-sunod ng magnitude na mas mabagal kaysa sa direktang koneksyon sa proseso; mas mura sa Linux/io_uring, ngunit
  Tumalon pa. Ang page matrix ay gumagamit ng store caliber, at ang runtime caliber ay ginagamit para sa "deployment form vs C++
  Paghahambing ng "Public Interface" (ang fkv runtime na single-thread reading ay humigit-kumulang 50 kops/s, C++ EloqStore FFI single-thread
  shard tungkol sa 21 kops/s).
- **Basahin ang concurrency caliber**: fkv side thread number = `min(readers, --read-max-concurrency)`, default
  Ang pinakamataas na limitasyon ay 4 (nakahanay sa evaluation machine vCPU), **hindi** "isang thread sa bawat reader" - ang nalampasan na antas ay nagpapataas lamang ng pila;
  Cache bawat thread = 256 MiB / bilang ng mga thread (ang kabuuang halaga ay naayos, hindi ginutay-gutay sa mga mambabasa), at ginamit
  `FuturesUnordered` Panatilihin ang `--read-window` (default 16) na mga kahilingan sa mabilisang. Ang bahagi ng RocksDB ay
  `readers` Bawat `std::thread` ay sabay na nagbabasa; single-threaded loop sa fjall side (hindi pinapansin ang gear ng mga mambabasa,
  Bilang reference column lamang). Maaaring bumaba ang `--no-fkv-read-threads` pabalik sa "interleaved N asynchronous chain sa isang solong runtime."
  Iyan ay pipeline depth, hindi concurrency, at hindi maihahambing sa mga engine.
- **len()**: Wala sa tatlong makina ang may O(1) na mga interface ng pagbibilang. Ang linyang ito ay ang buong rate ng pagbibilang ng talahanayan, hindi O(1) lookup.