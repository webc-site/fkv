# Výkon

`fkv_bench` Otestujte zaťaženie zapisovania, čítania a skenovania na pevnom kľúčovom priestore.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Medzi indikátory patrí priepustnosť, latencia v každej fáze a vrchol RSS. vydanie pracovného postupu vydanie x86_64 /
`fkv_bench` binárne pre aarch64.

## Trojstranný zápas s RocksDB/fjall

`crate/perf/fkv_compare` Použite rovnaký ovládač fázy a rovnaké kódovanie kľúča (8-bajtový big-endian dolný index na konci,
Poradie slovníka je v súlade s poradím dolného indexu) Test fkv, RocksDB, fjall: RocksDB úspešne prešiel
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Kompilovaná zdieľaná knižnica sa načíta pomocou dlopen (rovnaký C
ABI, Rust build nie je prepojený s RocksDB), fjall používa `fjall 3` priamo. Každý z troch motorov otvára knižnicu.
Postup v rovnakej knižnici v poradí etáp: hromadné načítanie vytvorí údaje a potom len / bodové čítanie / čítanie rozsahu /
Všetky vymazania ovplyvňujú rovnaké údaje.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Preskočte tretí stĺpec, ak tam nie je RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Čísla pre maticu domovskej stránky môžu pochádzať iba z jedného sledovateľného cyklu:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --stroj "..." --zdroj "Spustenie CI <id>"
```

`perf.yml` GitHub Actions robí presne toto (nainštalujte `librocksdb-dev`, naprogramujte adaptéry, spustite
maticu riadkov s veľkosťou 25 miliónov, odovzdať artefakt a priamo vyplniť stránku, keď `backfill=true`).

### kalibru

- **Pomer dát/vyrovnávacej pamäte**: 25 000 000 riadkov × 116 B ≈ 2 766 MiB údajov, 256 MiB vyrovnávacej pamäte na motor,
  ≈ 11:1, v súlade s oficiálnou recenziou RocksDB dát 97 GiB/6 GiB vyrovnávacej pamäte (≈ 16:1).
- **Rovnaké množstvo vyrovnávacej pamäte**: vyrovnávacia pamäť stránok fkv (rovnaký fond indexov/údajov), vyrovnávacia pamäť blokov RocksDB
  (`cache_index_and_filter_blocks=true`, veľkosť bloku 4 KiB), fjallova vlastná vyrovnávacia pamäť blokov
  Všetky rozpočty sú nastavené na rovnaký počet bajtov.
- **Kaliber perzistencie (stĺpec porovnávania používa predvolenú konfiguráciu RocksDB, bez zarovnania)**: fkv 1×stránka s údajmi + na dávku
  1×manifest `fdatasync` (Izomorfný k SyncData + AppendManifest z C++ EloqStore,
  Ani jeden nemá vypínač); stĺpec RocksDB používa **predvolenú konfiguráciu** (WAL zapnutá, `WriteOptions.sync=false`,
  Žiadny sekvenčný fsync), fjall tiež používa predvolené nastavenie (`PersistMode::Buffer`). Preto fáza zápisu (jeden zápis /
  Rozdiel medzi dávkovými zápismi) je **rozdiel perzistentnej sily**, nie rozdiel v implementácii, ktorý musí byť uvedený na stránke spolu s číslom.
  Ak chcete analyzovať „kto je rýchlejší pri rovnakej sile synchronizácie“, pridajte `--align-fsync` (otvorte RocksDB s sync=true,
  fjall používa SyncAll), táto možnosť sa nepoužíva pre externe publikované matice.
- **Dva ovládače fkv**: `--fkv-driver store` (predvolené, priamo pripojené k jednému `Store` v rámci procesu a
  RocksDB / fjall sú tiež volania knižnice) a `--fkv-driver runtime` (režim nasadenia: proces
  `--fkv-shards` zlomkové vlákna, požiadavky sú smerované podľa kľúča, zosúladené s verejným čítaním a zápisom C++ EloqStore
  rozhranie). Každá požiadavka druhého z nich musí prejsť frontom + prebudenie naprieč vláknami, natívne (macOS, IO súboru prechádza cez fond vlákien)
  Čas čítania je približne 20 µs/čas, čo je rádovo pomalšie ako priame pripojenie počas procesu; oveľa lacnejšie na Linuxe/io_uring, ale
  Ešte skok. Matica stránky používa kaliber obchodu a kaliber runtime sa používa pre „formu nasadenia vs C++
  Porovnanie "verejného rozhrania" (fkv runtime jednovláknové čítanie je asi 50 kops/s, C++ EloqStore FFI jednovláknové
  črep asi 21 kop/s).
- **Čítajte kalibru súbežnosti**: číslo vlákna na strane fkv = `min(readers, --read-max-concurrency)`, predvolené
  Horná hranica je 4 (zarovnaná s vyhodnocovacím strojom vCPU), **nie** „jedno vlákno na čítačku“ – prekročená úroveň iba zvyšuje front;
  Cache na vlákno = 256 MiB / počet vlákien (celkové množstvo je pevné, nie je skartované čítačkami) a použité
  `FuturesUnordered` Udržujte `--read-window` (predvolene 16) žiadostí za behu. RocksDB strana je
  `readers` Každý `std::thread` číta súčasne; jednovláknová slučka na strane fjall (ignoruje vybavenie čítačky,
  Len ako referenčný stĺpec). `--no-fkv-read-threads` sa môže zvrhnúť späť na "prekladané N asynchrónne reťazce v jednom runtime".
  To je hĺbka potrubia, nie súbežnosť a nie je porovnateľná medzi motormi.
- **len()**: Žiadny z troch motorov nemá rozhranie na počítanie O(1). Tento riadok predstavuje celkovú mieru počítania tabuľky, nie vyhľadávanie O(1).