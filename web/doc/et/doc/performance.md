# Jõudlus

`fkv_bench` Testige kirjutamise, lugemise ja skannimise koormusi fikseeritud võtmeruumil.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Näitajad hõlmavad läbilaskevõimet, latentsust igas etapis ja tipp-RSS-i. vabasta töövoo väljalase x86_64 /
`fkv_bench` binaarfail aarch64 jaoks.

## Kolmekordne kokkumäng RocksDB/fjalliga

`crate/perf/fkv_compare` Kasutage sama etapi draiverit ja sama võtme kodeeringut (8-baidine suur alamindeks lõpus,
Sõnastiku järjekord on kooskõlas alaindeksi järjekorraga) Test fkv, RocksDB, fjall: RocksDB läbitud
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Kompileeritud ühisteeki laaditakse dlopen (sama C
ABI, Rust build ei linki RocksDB-ga), fjall kasutab otse `fjall 3`. Kõik kolm mootorit avavad raamatukogu.
Liikuge edasi samas teegis etappide järjekorras: hulgilaadimine loob andmed ja seejärel len / punkti lugemine / vahemiku lugemine /
Kõik kustutamised mõjutavad samu andmeid.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Kui RocksDB puudub, jätke kolmas veerg vahele
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Kodulehe maatriksi numbrid võivad pärineda ainult ühest jälgitavast jooksust:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --masin "..." --source "CI run <id>"
```

GitHub Actionsi `perf.yml` teeb täpselt seda (installi `librocksdb-dev`, programmeerib adapterid, käivita
25M reamaatriks, laadige üles artefakt ja täitke leht otse, kui `backfill=true`).

### kaliibriga

- **Andmete/vahemälu suhe**: 25 000 000 rida × 116 B ≈ 2766 MiB andmeid, 256 MiB vahemälu mootori kohta,
  ≈ 11:1, mis on kooskõlas RocksDB ametliku ülevaatega 97 GiB andmete/6 GiB vahemälu kohta (≈ 16:1).
- **Sama vahemälu**: fkv lehe vahemälu (indeks/andmed sama kogum), RocksDB plokkvahemälu
  (`cache_index_and_filter_blocks=true`, ploki suurus 4 KiB), fjalli enda ploki vahemälu
  Kõik eelarved on seatud samale arvule baitidele.
- **Püsivuse kaliiber (võrdlusveerg kasutab RocksDB vaikekonfiguratsiooni ilma joondamiseta)**: fkv 1 × andmeleht + partii kohta
  1×manifest `fdatasync` (isomorfne SyncDatale + C++ EloqStore'i AppendManifest,
  kummalgi pole väljalülitit); veerg RocksDB kasutab **vaikekonfiguratsiooni** (WAL sees, `WriteOptions.sync=false`,
  Järjestikust fsynci pole), kasutab fjall ka vaikeseadet (`PersistMode::Buffer`). Seetõttu kirjutamisfaas (ühekordne kirjutamine /
  Erinevus partiikirjutuste vahel on **püsivuse tugevuse erinevus**, mitte teostuse erinevus, mis tuleb lehel märkida koos numbriga.
  Kui soovite analüüsida "kes on sama sünkroonimistugevuse juures kiirem", lisage `--align-fsync` (avage RocksDB koos sync=true,
  fjall kasutab SyncAll-i), seda valikut väliselt avaldatud maatriksite puhul ei kasutata.
- **Kaks fkv draiverit**: `--fkv-driver store` (vaikimisi, protsessi käigus otse ühendatud ühe `Store`-ga ja
  RocksDB / fjall on ka teegikutsed) ja `--fkv-driver runtime` (juurutusrežiim: protsess
  `--fkv-shards` killulõime, päringud suunatakse võtmega, joondatakse C++ EloqStore'i avaliku lugemise ja kirjutamisega
  liides). Viimase iga päring peab läbima järjekorra + lõimeülese äratuse, algne (macOS, faili IO läbib lõimekogumi)
  Lugemisaeg on umbes 20 µs/aeg, mis on suurusjärgu aeglasem kui protsessisisene otseühendus; palju odavam Linuxis/io_uringis, kuid
  Ikka hüpe. Lehe maatriks kasutab poekaliibrit ja käitusaja kaliibrit kasutatakse juurutamisvormi ja C++ jaoks.
  "Avaliku liidese" võrdlus (fkv käitusaja ühe lõime näit on umbes 50 kops/s, C++ EloqStore FFI ühelõimeline
  kild umbes 21 kops/s).
- **Lugege samaaegsuse kaliibrit**: fkv küljelõime number = `min(readers, --read-max-concurrency)`, vaikimisi
  Ülempiir on 4 (joondatud hindamismasina vCPU-ga), **mitte** "üks lõim lugeja kohta" – ületatud tase suurendab ainult järjekorda;
  Vahemälu lõime kohta = 256 MiB / lõimede arv (kogusumma on fikseeritud, pole lugejatega purustatud) ja kasutatud
  `FuturesUnordered` Hoidke `--read-window` (vaikimisi 16) taotlusi jooksvalt. RocksDB pool on
  `readers` Iga `std::thread` loeb samaaegselt; ühe keermega silmus fjall poolel (lugejate käiku eirates,
  Ainult võrdlusveeruna). `--no-fkv-read-threads` võib degenereeruda tagasi "põimitud N asünkroonseks ahelaks ühel käitusajal".
  See on torujuhtme sügavus, mitte samaaegsus ja see pole mootorite vahel võrreldav.
- **len()**: ühelgi kolmest mootorist pole O(1) loendusliidest. See rida on kogu tabeli loendusmäär, mitte O(1) otsing.