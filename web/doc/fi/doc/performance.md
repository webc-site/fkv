# Suorituskyky

`fkv_bench` Testaa kirjoitus-, luku- ja skannauskuormia kiinteällä näppäinalueella.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikaattorit sisältävät suorituskyvyn, latenssin kussakin vaiheessa ja huippu-RSS:n. vapauta työnkulku julkaisu x86_64 /
`fkv_bench` binääri aarch64:lle.

## Kolmisuuntainen ottelu RocksDB/fjallin kanssa

`crate/perf/fkv_compare` Käytä samaa vaiheohjainta ja samaa avainkoodausta (8-tavun suuri alaindeksi lopussa,
Sanakirjan järjestys on yhdenmukainen alaindeksien järjestyksen kanssa) Testi fkv, RocksDB, fjall: RocksDB hyväksytty
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Käytetty jaettu kirjasto ladataan dlopenilla (sama C
ABI, Rust build ei linkitä RocksDB:hen), fjall käyttää `fjall 3` suoraan. Jokainen kolmesta moottorista avaa kirjaston.
Etene samassa kirjastossa vaihejärjestyksessä: joukkolataus luo dataa ja sitten len / pisteen lukeminen / etäisyyden lukeminen /
Kaikki poistot vaikuttavat samoihin tietoihin.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Ohita kolmas sarake, jos RocksDB:tä ei ole
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Kotisivumatriisin numerot voivat tulla vain yhdestä jäljitettävästä ajosta:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --kone "..." --lähde "CI-ajo <id>"
```

GitHub Actionsin `perf.yml` tekee juuri tämän (asenna `librocksdb-dev`, ohjelmoi sovittimet, suorita
25M rivimatriisi, lähetä artefakti ja täytä sivu suoraan, kun `backfill=true`).

### kaliiperi

- **Tiedon/välimuistin suhde**: 25 000 000 riviä × 116 B ≈ 2766 MiB tietoa, 256 MiB välimuistia moottoria kohti,
  ≈ 11:1, linjassa RocksDB:n virallisen 97 GiB:n datan / 6 GiB:n välimuistin katsauksen kanssa (≈ 16:1).
- **Sama määrä välimuistia**: fkv:n sivuvälimuisti (indeksi/data sama pooli), RocksDB:n lohkovälimuisti
  (`cache_index_and_filter_blocks=true`, lohkokoko 4 KiB), fjallin oma lohkovälimuisti
  Kaikki budjetit on asetettu samalle tavumäärälle.
- **Pysyvyyskaliiperi (vertailusarake käyttää RocksDB:n oletuskokoonpanoa ilman kohdistusta)**: fkv 1×tietosivu + erää kohti
  1×manifest `fdatasync` (C++ EloqStoren isomorfinen SyncData + AppendManifest,
  Kummassakaan ei ole sammutuskytkintä); RocksDB-sarake käyttää **oletuskokoonpanoa** (WAL päällä, `WriteOptions.sync=false`,
  Ei peräkkäistä fsyncia, fjall käyttää myös oletusarvoa (`PersistMode::Buffer`). Siksi kirjoitusvaihe (yksi kirjoitus /
  Ero eräkirjoitusten välillä) on **pysyvyysvoimaero**, ei toteutusero, joka on ilmoitettava sivulla numeron kanssa.
  Jos haluat analysoida "kuka on nopeampi samalla synkronointivoimakkuudella", lisää `--align-fsync` (avaa RocksDB komennolla sync=true,
  fjall käyttää SyncAll-toimintoa), tätä vaihtoehtoa ei käytetä ulkoisesti julkaistuille matriiseille.
- **Kaksi fkv-ohjainta**: `--fkv-driver store` (oletus, yhdistetty suoraan yhteen `Store` prosessin aikana, ja
  RocksDB / fjall ovat myös kirjastokutsuja) ja `--fkv-driver runtime` (käyttöönottotila: prosessi
  `--fkv-shards` sirpaleet, pyynnöt reititetään avaimella, kohdistetaan C++ EloqStoren julkiseen lukemiseen ja kirjoittamiseen
  käyttöliittymä). Jälkimmäisen jokaisen pyynnön on läpäistävä jono + säikeiden välinen herätys, natiivi (macOS, tiedosto IO kulkee säievarannon läpi)
  Lukuaika on noin 20 µs/aika, mikä on suuruusluokkaa hitaampi kuin prosessin sisäinen suora yhteys; paljon halvempaa Linuxissa/io_uringissa, mutta
  Hyppy silti. Sivumatriisi käyttää myymäläkaliiperia, ja ajonaikaista kaliiperia käytetään "käyttöönottolomake vs C++
  "Public Interface" -vertailu (fkv-ajonaikainen yksisäikeinen luku on noin 50 kops/s, C++ EloqStore FFI yksisäikeinen
  sirpale noin 21 kops/s).
- **Lue samanaikaisuuskaliiperi**: fkv sivulangan numero = `min(readers, --read-max-concurrency)`, oletus
  Yläraja on 4 (linjattu arviointikoneen vCPU:n kanssa), **ei** "yksi säiettä lukijaa kohti" - ylitetty taso vain lisää jonoa;
  Välimuisti säiettä kohden = 256 MiB / säikeiden lukumäärä (kokonaismäärä on kiinteä, ei silputtu lukijoiden kanssa) ja käytetty
  `FuturesUnordered` Pidä `--read-window` (oletus 16) pyynnöt lennossa. RocksDB puoli on
  `readers` Jokainen `std::thread` lukee samanaikaisesti; yksisäikeinen silmukka fjall-puolella (huolimatta lukijan varusteet,
  Vain vertailusarakkeena). `--no-fkv-read-threads` voi degeneroitua takaisin "lomitetuksi N asynkroniseksi ketjuksi yhdessä suoritusaikana".
  Se on putkilinjan syvyys, ei samanaikaisuus, eikä sitä voida verrata moottoreiden välillä.
- **len()**: Yhdelläkään kolmesta moottorista ei ole O(1)-laskentaliitäntää. Tämä rivi on koko taulukon laskentanopeus, ei O(1)-haku.