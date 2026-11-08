# Zmogljivost

`fkv_bench` Preskusno pisanje, branje in pregledovanje obremenitev na fiksnem ključnem prostoru.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Kazalniki vključujejo prepustnost, zakasnitev na vsaki stopnji in najvišjo vrednost RSS. izdaja izdaje poteka dela x86_64 /
`fkv_bench` binarno za aarch64.

## Tristransko ujemanje z RocksDB/fjall

`crate/perf/fkv_compare` Uporabite isti stopenjski gonilnik in isto kodiranje ključev (8-bajtni indeks velikega endija na koncu,
Vrstni red v slovarju je skladen z vrstnim redom indeksov) Test fkv, RocksDB, fjall: RocksDB uspešno
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Prevedena knjižnica v skupni rabi je naložena z dlopen (isti C
ABI, zgradba Rust se ne povezuje z RocksDB), fjall neposredno uporablja `fjall 3`. Vsak od treh motorjev odpre knjižnico.
Napredujte v isti knjižnici po vrstnem redu stopenj: množično nalaganje ustvari podatke in nato len / branje točk / branje obsega /
Vsi izbrisi vplivajo na iste podatke.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Preskoči tretji stolpec, če ni RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Številke za matriko domače strani lahko izvirajo samo iz enega sledljivega zagona:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --stroj "..." --source "CI zagon <id>"
```

`perf.yml` GitHub Actions naredi točno to (namesti `librocksdb-dev`, programske adapterje, zaženi
25M matriko vrstic, naložite artefakt in neposredno zapolnite stran, ko `backfill=true`).

### kalibra

- **Razmerje podatkov/predpomnilnika**: 25.000.000 vrstic × 116 B ≈ 2766 MiB podatkov, 256 MiB predpomnilnika na motor,
  ≈ 11:1, usklajeno z uradnim pregledom RocksDB podatkov 97 GiB/6 GiB predpomnilnika (≈ 16:1).
- **Enaka količina predpomnilnika**: predpomnilnik strani fkv (indeks/podatki isti bazen), blokovni predpomnilnik RocksDB
  (`cache_index_and_filter_blocks=true`, velikost bloka 4 KiB), fjallov lastni predpomnilnik blokov
  Vsi proračuni so nastavljeni na enako število bajtov.
- **Kaliber vztrajnosti (primerjalni stolpec uporablja privzeto konfiguracijo RocksDB, brez poravnave)**: fkv 1×podatkovna stran + na paket
  1×manifest `fdatasync` (izomorfen SyncData + AppendManifest C++ EloqStore,
  Niti nima stikala za izklop); stolpec RocksDB uporablja **privzeto konfiguracijo** (WAL vklopljen, `WriteOptions.sync=false`,
  Brez zaporednega fsync), fjall uporablja tudi privzeto (`PersistMode::Buffer`). Zato je faza pisanja (enojno pisanje /
  Razlika med paketnim pisanjem) je **razlika v moči obstojnosti** in ne razlika v implementaciji, ki mora biti navedena na strani skupaj s številko.
  Če želite analizirati, "kdo je hitrejši pri enaki moči sinhronizacije", dodajte `--align-fsync` (odprite RocksDB s sync=true,
  fjall uporablja SyncAll), ta možnost se ne uporablja za zunanje objavljene matrike.
- **Dva gonilnika fkv**: `--fkv-driver store` (privzeto, neposredno povezan z enim `Store` znotraj procesa in
  RocksDB / fjall sta tudi klica knjižnice) in `--fkv-driver runtime` (način uvajanja: proces
  `--fkv-shards` niti drobcev, zahteve so usmerjene po ključu, usklajene z javnim branjem in pisanjem C++ EloqStore
  vmesnik). Vsaka zahteva slednjega mora iti skozi čakalno vrsto + bujenje med nitmi, izvirno (macOS, datoteka IO gre skozi skupino niti)
  Čas branja je približno 20 µs/čas, kar je za red velikosti počasneje od neposredne povezave med procesom; veliko cenejši na Linux/io_uring, vendar
  Še vedno skok. Matrika strani uporablja kaliber trgovine, kaliber izvajalnega časa pa se uporablja za "razmestitveni obrazec proti C++
  Primerjava "javnega vmesnika" (fkv runtime enonitno branje je približno 50 kops/s, C++ EloqStore FFI enonit
  drobec približno 21 kops/s).
- **Brani kaliber sočasnosti**: številka stranske niti fkv = `min(readers, --read-max-concurrency)`, privzeto
  Zgornja meja je 4 (usklajena z vCPU ocenjevalnega stroja), **ne** "ena nit na bralnik" - presežena raven samo poveča čakalno vrsto;
  Predpomnilnik na nit = 256 MiB / število niti (skupna količina je fiksna, ni razrezana z bralniki) in uporabljena
  `FuturesUnordered` Ohranite `--read-window` (privzeto 16) zahtev sproti. Stran RocksDB je
  `readers` Vsak `std::thread` bere hkrati; enonitna zanka na strani fjall (brez upoštevanja opreme za branje,
  Samo kot referenčni stolpec). `--no-fkv-read-threads` lahko degenerira nazaj v "prepletene N asinhronih verig v enem samem izvajalnem okolju".
  To je globina cevovoda, ne sočasnost, in ni primerljiva med motorji.
- **len()**: Nobeden od treh motorjev nima vmesnikov za štetje O(1). Ta vrstica je stopnja štetja celotne tabele, ne iskanje O(1).