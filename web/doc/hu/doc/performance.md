# Teljesítmény

`fkv_bench` Tesztelje az írási, olvasási és szkennelési terheléseket egy rögzített kulcsterületen.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

A mutatók közé tartozik az átviteli sebesség, a késleltetés az egyes szakaszokban és a csúcs RSS. kiadás munkafolyamat kiadás x86_64 /
`fkv_bench` bináris az aarch64 számára.

## Háromesélyes mérkőzés a RocksDB/fjall-lal

`crate/perf/fkv_compare` Használja ugyanazt a szakaszos illesztőprogramot és ugyanazt a kulcskódolást (8 bájtos big endian alsó index a végén,
A szótár sorrendje összhangban van az alsó index sorrendjével) Teszt fkv, RocksDB, fjall: RocksDB sikeres
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` A lefordított megosztott könyvtár dlopennel van betöltve (ugyanaz a C
ABI, a Rust build nem hivatkozik a RocksDB-re), az fjall közvetlenül az `fjall 3`-t használja. A három motor mindegyike megnyit egy könyvtárat.
Továbblépés ugyanabban a könyvtárban a szakaszok sorrendjében: a tömeges betöltés adatokat hoz létre, majd len / pont olvasás / tartomány olvasása /
A törlések ugyanazokat az adatokat érintik.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Ha nincs RocksDB, hagyja ki a harmadik oszlopot
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

A kezdőlap mátrixának számai csak egyetlen nyomon követhető futtatásból származhatnak:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --forrás "CI futtatása <id>"
```

A GitHub Actions `perf.yml` pontja ezt teszi (telepítse az `librocksdb-dev`t, programozza az adaptereket, futtassa
25 millió soros mátrix, műtermék feltöltése, és közvetlenül az oldal háttérkitöltése, ha `backfill=true`).

### kaliber

- **Adat/gyorsítótár arány**: 25 000 000 sor × 116 B ≈ 2766 MiB adat, 256 MiB gyorsítótár motoronként,
  ≈ 11:1, igazodik a RocksDB 97 GiB adat/6 GiB gyorsítótár hivatalos áttekintéséhez (≈ 16:1).
- **Ugyanannyi gyorsítótár**: az fkv oldal gyorsítótára (index/adatkészlet azonos), a RocksDB blokk gyorsítótára
  (`cache_index_and_filter_blocks=true`, blokkméret 4 KiB), az fjall saját blokk-gyorsítótára
  A költségvetések mindegyike azonos számú bájtra van beállítva.
- **Perzisztencia kaliber (az összehasonlító oszlop a RocksDB alapértelmezett konfigurációját használja, igazítás nélkül)**: fkv 1×adatoldal + kötegenként
  1×manifest `fdatasync` (Izomorf a SyncData-hoz + AppendManifest a C++ EloqStore-hoz,
  egyikben sincs kikapcsoló); a RocksDB oszlop az **alapértelmezett konfigurációt** használja (WAL be, `WriteOptions.sync=false`,
  Nincs szekvenciális fsync), az fjall szintén az alapértelmezettet (`PersistMode::Buffer`) használja. Ezért az írási fázis (egyszeri írás /
  A kötegelt írások közötti különbség a **perzisztencia-erőkülönbség**, nem a megvalósítási különbség, amit a számmal együtt fel kell tüntetni az oldalon.
  Ha azt szeretné elemezni, hogy "ki a gyorsabb azonos szinkronizálási erősség mellett", adja hozzá az `--align-fsync` elemet (nyissa meg a RocksDB-t sync=true,
  Az fjall a SyncAll-t használja), ez a beállítás nem használható külsőleg közzétett mátrixokhoz.
- **Két fkv-illesztőprogram**: `--fkv-driver store` (alapértelmezett, közvetlenül kapcsolódik egyetlen `Store`-hez a folyamaton belül, és
  A RocksDB / fjall szintén könyvtárhívások) és `--fkv-driver runtime` (telepítési mód: folyamat
  `--fkv-shards` szilánkos szálak, a kérések kulcs által irányítva, a C++ EloqStore nyilvános olvasásához és írásához igazítva
  felület). Ez utóbbi minden kérésének át kell mennie a sorban + keresztszál-ébresztésen, natív (macOS, az IO fájl átmegy a szálkészleten)
  Az olvasási idő körülbelül 20 µs/idő, ami egy nagyságrenddel lassabb, mint a folyamat közbeni közvetlen kapcsolat; sokkal olcsóbb Linuxon/io_uringon, de
  Még mindig ugrás. Az oldalmátrix az áruház kaliberét használja, a futásidejű kalibert pedig a „telepítési forma vs C++
  A "Public Interface" összehasonlítása (fkv futásidejű egyszálas olvasási sebesség kb. 50 kops/s, C++ EloqStore FFI egyszál
  szilánk körülbelül 21 kops/s).
- **A párhuzamossági kaliber olvasása**: fkv oldalsó szál száma = `min(readers, --read-max-concurrency)`, alapértelmezett
  A felső határ 4 (a kiértékelő gép vCPU-jához igazítva), **nem** "egy szál olvasónként" - a túllépés csak növeli a sort;
  Gyorsítótár szálanként = 256 MiB / szálak száma (a teljes összeg rögzített, nem aprított olvasókkal), és felhasználva
  `FuturesUnordered` A `--read-window` (alapértelmezett 16) kérések menet közben tartása. A RocksDB oldala az
  `readers` Mindegyik `std::thread` egyszerre olvasható; egyszálú hurok az fjall oldalon (figyelmen kívül hagyva az olvasó felszerelését,
  Csak referencia oszlopként). A `--no-fkv-read-threads` visszafajulhat "egyetlen futásidejű N interleaved aszinkron láncra".
  Ez a csővezeték mélysége, nem az egyidejűség, és nem hasonlítható össze a motorok között.
- **len()**: A három motor egyike sem rendelkezik O(1) számláló interfésszel. Ez a sor a teljes táblázat számlálási sebessége, nem az O(1) keresés.