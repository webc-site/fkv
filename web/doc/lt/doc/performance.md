# Našumas

`fkv_bench` Išbandykite rašymo, skaitymo ir nuskaitymo apkrovas fiksuotoje klavišų erdvėje.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Rodikliai apima pralaidumą, delsą kiekviename etape ir didžiausią RSS. išleidimo darbo eigos leidimas x86_64 /
`fkv_bench` dvejetainis, skirtas aarch64.

## Trijų krypčių rungtynės su RocksDB/fjall

`crate/perf/fkv_compare` Naudokite tą pačią pakopos tvarkyklę ir tą pačią rakto kodavimą (8 baitų didelis apatinis indeksas pabaigoje,
Žodyno tvarka atitinka indekso tvarką) Bandymas fkv, RocksDB, fjall: RocksDB išlaikytas
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Sukompiliuota bendrinama biblioteka įkeliama su dlopen (ta pati C
ABI, Rust build nesusieja su RocksDB), fjall tiesiogiai naudoja `fjall 3`. Kiekvienas iš trijų variklių atidaro biblioteką.
Pereikite prie tos pačios bibliotekos etapų tvarka: masinis įkėlimas sukuria duomenis, o tada len / taško nuskaitymas / diapazono skaitymas /
Visi ištrynimai turi įtakos tiems patiems duomenims.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Praleiskite trečią stulpelį, jei nėra RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Pagrindinio puslapio matricos skaičiai gali būti gauti tik iš vieno atsekamo paleidimo:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --mašina "..." --šaltinis "CI paleisti <id>"
```

„GitHub Actions“ `perf.yml` atlieka būtent tai (įdiegti `librocksdb-dev`, programuoti adapterius, paleisti
25M eilučių matrica, įkelkite artefaktą ir tiesiogiai užpildykite puslapį, kai `backfill=true`).

### kalibro

- **Duomenų / talpyklos santykis**: 25 000 000 eilučių × 116 B ≈ 2766 MiB duomenų, 256 MiB talpyklos vienam varikliui,
  ≈ 11:1, suderinta su oficialia RocksDB 97 GiB duomenų / 6 GiB talpyklos apžvalga (≈ 16:1).
- **Tas pats talpyklos kiekis**: fkv puslapio talpykla (indeksas / duomenys tas pats telkinys), RocksDB blokinė talpykla
  (`cache_index_and_filter_blocks=true`, bloko dydis 4 KiB), paties fjall bloko talpykla
  Visi biudžetai nustatyti į tą patį baitų skaičių.
- **Pasilikimo kalibras (palyginimo stulpelyje naudojama numatytoji RocksDB konfigūracija, be lygiavimo)**: fkv 1 × duomenų puslapis + vienai paketai
  1×manifestas `fdatasync` (izomorfinis C++ EloqStore SyncData + AppendManifest,
  nei vienas neturi išjungimo jungiklio); stulpelyje RocksDB naudojama **numatytoji konfigūracija** (WAL įjungta, `WriteOptions.sync=false`,
  Nėra nuoseklaus fsync), fjall taip pat naudoja numatytąjį (`PersistMode::Buffer`). Todėl rašymo fazė (vienas rašymas /
  Skirtumas tarp partijos rašymo) yra **patvarumo stiprumo skirtumas**, o ne įgyvendinimo skirtumas, kuris turi būti nurodytas puslapyje kartu su skaičiumi.
  Jei norite išanalizuoti „kas yra greitesnis esant tokiam pačiam sinchronizavimo stiprumui“, pridėkite `--align-fsync` (atidarykite RocksDB su sync=true,
  fjall naudoja SyncAll), ši parinktis nenaudojama išorėje paskelbtoms matricoms.
- **Dvi fkv tvarkyklės**: `--fkv-driver store` (numatytasis, proceso metu tiesiogiai prijungtas prie vieno `Store` ir
  RocksDB / fjall taip pat yra bibliotekos iškvietimai) ir `--fkv-driver runtime` (diegimo režimas: procesas
  `--fkv-shards` fragmentų gijos, užklausos nukreipiamos pagal raktą, suderintos su viešu C++ EloqStore skaitymu ir rašymu
  sąsaja). Kiekviena pastarojo užklausa turi eiti per eilę + kryžminės gijos pažadinimas, gimtoji (macOS, failo IO eina per gijų telkinį)
  Skaitymo laikas yra apie 20 µs/laikas, o tai yra eilės tvarka lėtesnis nei proceso metu vykstantis tiesioginis ryšys; daug pigiau Linux/io_uring, bet
  Vis tiek šuolis. Puslapio matricoje naudojamas parduotuvės kalibras, o vykdymo laiko kalibras naudojamas „diegimo formai prieš C++
  „Viešosios sąsajos“ palyginimas (fkv vykdymo laiko vienos gijos skaitymas yra apie 50 kops/s, C++ EloqStore FFI vienos gijos skaitymas
  skeveldra apie 21 kop/s).
- **Skaityti lygiagretumo kalibrą**: fkv šoninės gijos numeris = `min(readers, --read-max-concurrency)`, numatytasis
  Viršutinė riba yra 4 (sulyginta su vertinimo mašinos vCPU), **ne** "viena gija skaitytuvui" - viršytas lygis tik padidina eilę;
  Talpykla gijoje = 256 MiB / gijų skaičius (bendra suma yra fiksuota, nesusmulkinta su skaitytuvais) ir naudojama
  `FuturesUnordered` Saugokite `--read-window` (numatytasis 16) užklausas. RocksDB pusė yra
  `readers` Kiekvienas `std::thread` skaitomas vienu metu; vieno sriegio kilpa fjall pusėje (nepaisant skaitytuvo įrangos,
  Tik kaip nuorodos stulpelį). `--no-fkv-read-threads` gali išsigimti atgal į "perlipusias N asinchronines grandines vienu vykdymo laiku".
  Tai yra dujotiekio gylis, o ne lygiagretumas ir jo negalima palyginti su įvairiais varikliais.
- **len()**: nė vienas iš trijų variklių neturi O(1) skaičiavimo sąsajų. Ši eilutė yra visos lentelės skaičiavimo greitis, o ne O(1) paieška.