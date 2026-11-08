# Performans

`fkv_bench` Sabit açar məkanında yükləri yazmaq, oxumaq və skan etmək üçün test edin.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Göstəricilərə ötürmə qabiliyyəti, hər mərhələdə gecikmə və pik RSS daxildir. iş axını buraxın x86_64 /
aarch64 üçün `fkv_bench` binar.

## RocksDB/fjall ilə üçtərəfli uyğunluq

`crate/perf/fkv_compare` Eyni mərhələ sürücüsündən və eyni açar kodlaşdırmasından istifadə edin (sonda 8 baytlıq böyük endian alt simvolu,
Lüğət sırası alt yazı sırasına uyğundur) Test fkv, RocksDB, fjall: RocksDB keçdi
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Tərtib edilmiş ortaq kitabxana dlopen ilə yüklənir (eyni C
ABI, Rust quruluşu RocksDB ilə əlaqə saxlamır), fjall birbaşa `fjall 3` istifadə edir. Üç mühərrikin hər biri bir kitabxana açır.
Mərhələlər sırası ilə eyni kitabxanada irəliləyin: toplu yükləmə məlumatları yaradır, sonra len / point oxu / diapazon oxu /
Silinmələrin hamısı eyni dataya təsir edir.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # RocksDB yoxdursa üçüncü sütunu keçin
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Əsas səhifə matrisi üçün rəqəmlər yalnız bir izlənilə bilən qaçışdan gələ bilər:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --mənbə "CI run <id>"
```

GitHub Actions' `perf.yml` məhz bunu edir (`librocksdb-dev` quraşdırın, proqram adapterlərini quraşdırın, işə salın
25M sıra matrisi, artefakt yükləyin və `backfill=true` olduqda səhifəni birbaşa doldurun.

### kalibrli

- **Məlumat/Keş nisbəti**: 25.000.000 sıra × 116 B ≈ 2766 MiB məlumat, hər mühərrik üçün 256 MiB keş,
  ≈ 11:1, RocksDB-nin 97 GiB data/6 GiB keş (≈ 16:1) üzrə rəsmi icmalı ilə uyğunlaşdırılıb.
- **Eyni miqdarda keş**: fkv-nin səhifə keşi (indeks/məlumat eyni hovuz), RocksDB-nin blok keşi
  (`cache_index_and_filter_blocks=true`, blok ölçüsü 4 KiB), fjall-ın öz blok keşi
  Büdcələrin hamısı eyni bayt sayına təyin edilmişdir.
- **Dözümlülük kalibri (müqayisə sütunu RocksDB defolt konfiqurasiyasından istifadə edir, düzülmədən)**: fkv 1×məlumat səhifəsi + hər partiya üçün
  1×manifest `fdatasync` (SyncData-ya izomorf + C++ EloqStore-un AppendManifest,
  Heç birində söndürmə açarı yoxdur); RocksDB sütunu **defolt konfiqurasiyadan** istifadə edir (WAL on, `WriteOptions.sync=false`,
  Ardıcıl fsync yoxdur), fjall də defoltdan istifadə edir (`PersistMode::Buffer`). Buna görə yazma mərhələsi (tək yazma /
  Toplu yazılar arasındakı fərq) nömrə ilə birlikdə səhifədə qeyd edilməli olan icra fərqi deyil, **dözümlülük gücü fərqidir**.
  "Eyni sinxronizasiya gücü altında kimin daha sürətli olduğunu" təhlil etmək istəyirsinizsə, `--align-fsync` əlavə edin (RocksDB-ni sync=true ilə açın,
  fjall SyncAll-dan istifadə edir), bu seçim xaricdə dərc edilmiş matrislər üçün istifadə edilmir.
- **İki fkv sürücüsü**: `--fkv-driver store` (defolt, proses daxilində birbaşa tək `Store` ilə bağlıdır və
  RocksDB / fjall də kitabxana zəngləridir) və `--fkv-driver runtime` (yerləşdirmə rejimi: proses
  `--fkv-shards` parçalı mövzular, sorğular C++ EloqStore-un ictimai oxunması və yazılması ilə uyğunlaşdırılmış açarla yönləndirilir
  interfeys). Sonuncunun hər sorğusu növbədən keçməlidir + çarpaz iplik oyanışı, yerli (macOS, IO faylı mövzu hovuzundan keçir)
  Oxuma vaxtı təqribən 20 µs/zamandır, bu, prosesdə birbaşa əlaqədən daha yavaş miqyasdadır; Linux/io_uring-də daha ucuzdur, lakin
  Hələ bir atlama. Səhifə matrisi mağaza çapından istifadə edir və iş vaxtı çapı "yerləşdirmə forması və C++" üçün istifadə olunur.
  "İctimai İnterfeys"in müqayisəsi (fkv işləmə müddətinin tək başlıq oxunması təxminən 50 kops/s, C++ EloqStore FFI tək iplikdir.
  parça təxminən 21 qəpik/s).
- **Oxumaq paralellik kalibr**: fkv yan ip nömrəsi = `min(readers, --read-max-concurrency)`, defolt
  Yuxarı limit 4-dür (qiymətləndirmə maşını vCPU ilə uyğunlaşdırılmışdır), ** deyil** "hər oxucuya bir iplik" - aşılan səviyyə yalnız növbəni artırır;
  Başlıq başına keş = 256 MiB / iplərin sayı (ümumi məbləğ sabitdir, oxucularla parçalanmır) və istifadə olunur
  `FuturesUnordered` `--read-window` (defolt 16) sorğuları anında saxlayın. RocksDB tərəfidir
  `readers` Hər `std::thread` eyni vaxtda oxuyur; fjall tərəfində tək yivli döngə (oxucuların dişlilərinə məhəl qoymadan,
  Yalnız istinad sütunu kimi). `--no-fkv-read-threads` "tək icra müddətində interleaved N asinxron zəncir"ə geri dönə bilər.
  Bu boru kəmərinin dərinliyidir, paralellik deyil və mühərriklər arasında müqayisə oluna bilməz.
- **len()**: Üç mühərrikin heç birində O(1) sayma interfeysi yoxdur. Bu sətir O(1) axtarışı deyil, tam cədvəl hesablama dərəcəsidir.