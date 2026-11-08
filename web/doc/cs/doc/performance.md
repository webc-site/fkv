# Výkon

`fkv_bench` Testujte zápis, čtení a skenování na pevném prostoru klíče.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikátory zahrnují propustnost, latenci v každé fázi a vrchol RSS. vydání pracovního postupu vydání x86_64 /
`fkv_bench` binární pro aarch64.

## Třícestný matchup s RocksDB/fjall

`crate/perf/fkv_compare` Použijte stejný ovladač fáze a stejné kódování klíče (8bajtový dolní index big-endian na konci,
Pořadí slovníku je v souladu s pořadím dolního indexu) Test fkv, RocksDB, fjall: RocksDB prošel
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Kompilovaná sdílená knihovna je načtena pomocí dlopen (stejný C
ABI, Rust build nenavazuje na RocksDB), fjall přímo používá `fjall 3`. Každý ze tří motorů otevírá knihovnu.
Pokračovat ve stejné knihovně v pořadí fází: hromadné načtení vytvoří data a poté len / bodové čtení / čtení rozsahu /
Všechna smazání ovlivní stejná data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Přeskočte třetí sloupec, pokud tam není RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Čísla pro matici domovské stránky mohou pocházet pouze z jednoho sledovatelného běhu:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --stroj "..." --source "spuštění CI <id>"
```

`perf.yml` GitHub Actions dělá přesně toto (nainstalujte `librocksdb-dev`, naprogramujte adaptéry, spusťte
matice řádků 25 milionů, nahrání artefaktu a přímé vyplnění stránky, když `backfill=true`).

### ráže

- **Poměr dat/mezipaměti**: 25 000 000 řádků × 116 B ≈ 2 766 MiB dat, 256 MiB mezipaměti na motor,
  ≈ 11:1, v souladu s oficiální recenzí RocksDB dat 97 GiB/6 GiB mezipaměti (≈ 16:1).
- **Stejné množství mezipaměti**: mezipaměť stránek fkv (index/data stejný fond), mezipaměť bloků RocksDB
  (`cache_index_and_filter_blocks=true`, velikost bloku 4 KiB), vlastní mezipaměť bloků společnosti fjall
  Všechny rozpočty jsou nastaveny na stejný počet bajtů.
- **Kalibr perzistence (srovnávací sloupec používá výchozí konfiguraci RocksDB, bez zarovnání)**: fkv 1×stránka dat + na dávku
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Ani jeden nemá vypínač); sloupec RocksDB používá **výchozí konfiguraci** (WAL zapnuto, `WriteOptions.sync=false`,
  Žádné sekvenční fsync), fjall také používá výchozí (`PersistMode::Buffer`). Proto fáze zápisu (jediný zápis /
  Rozdíl mezi dávkovými zápisy) je **rozdíl perzistenční síly**, nikoli rozdíl v implementaci, který musí být uveden na stránce spolu s číslem.
  Pokud chcete analyzovat „kdo je rychlejší při stejné síle synchronizace“, přidejte `--align-fsync` (otevřete RocksDB s sync=true,
  fjall používá SyncAll), tato volba se nepoužívá pro externě publikované matice.
- **Dva ovladače fkv**: `--fkv-driver store` (výchozí, přímo připojené k jednomu `Store` v rámci procesu a
  RocksDB / fjall jsou také volání knihovny) a `--fkv-driver runtime` (režim nasazení: proces
  `--fkv-shards` shard vlákna, požadavky jsou směrovány podle klíče, v souladu s veřejným čtením a zápisem C++ EloqStore
  rozhraní). Každý požadavek posledně jmenovaného musí projít frontou + probuzení napříč vlákny, nativní (macOS, vstup souboru prochází fondem vláken)
  Doba čtení je asi 20 µs/čas, což je řádově pomalejší než přímé připojení v procesu; mnohem levnější na Linux/io_uring, ale
  Ještě skok. Matice stránek používá kalibr úložiště a kalibrace runtime se používá pro „formu nasazení vs C++
  Porovnání "veřejného rozhraní" (fkv runtime jednovláknové čtení je asi 50 kops/s, C++ EloqStore FFI jednovláknové
  střep asi 21 kop/s).
- **Čtení kalibru souběžnosti**: číslo vlákna strany fkv = `min(readers, --read-max-concurrency)`, výchozí
  Horní limit je 4 (v souladu s vyhodnocovacím strojem vCPU), **ne** "jedno vlákno na čtečku" - překročená úroveň pouze zvyšuje frontu;
  Cache na vlákno = 256 MiB / počet vláken (celkové množství je pevné, není skartováno čtečkami) a využito
  `FuturesUnordered` Udržujte `--read-window` (výchozí 16) požadavků za běhu. Strana RocksDB je
  `readers` Každý `std::thread` čte současně; jednovláknová smyčka na straně fjall (ignoruje vybavení čtečky,
  Pouze jako referenční sloupec). `--no-fkv-read-threads` se může zvrhnout zpět na "prokládaných N asynchronních řetězců v jednom běhovém prostředí".
  To je hloubka potrubí, ne souběžnost, a není srovnatelná napříč motory.
- **len()**: Žádný ze tří motorů nemá rozhraní pro počítání O(1). Tento řádek představuje míru počítání celé tabulky, nikoli vyhledávání O(1).