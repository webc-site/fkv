# Performans

`fkv_bench` Sabit bir anahtar alanında yazma, okuma ve tarama yüklerini test edin.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Göstergeler arasında aktarım hızı, her aşamadaki gecikme ve en yüksek RSS bulunur. iş akışı sürümünü yayın x86_64 /
`fkv_bench` aarch64 için ikili dosya.

## RocksDB/fjall ile üçlü eşleşme

`crate/perf/fkv_compare` Aynı aşama sürücüsünü ve aynı anahtar kodlamayı kullanın (sonda 8 baytlık büyük endian alt simgesi,
Sözlük sırası abone sırası ile tutarlıdır) Test fkv, RocksDB, fjall: RocksDB başarılı
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Derlenmiş paylaşılan kitaplık dlopen ile yüklenir (aynı C
ABI, Rust yapısı RocksDB'ye bağlanmaz), fjall doğrudan `fjall 3` kullanır. Üç motorun her biri bir kütüphane açar.
Aynı kitaplıkta aşama sırasına göre ilerleyin: toplu yükleme verileri oluşturur ve ardından uzunluk / nokta okuma / aralık okuma /
Silme işlemlerinin tümü aynı verileri etkiler.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # RocksDB yoksa üçüncü sütunu atla
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Ana sayfa matrisinin sayıları yalnızca tek bir izlenebilir çalışmadan gelebilir:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI çalıştırması <id>"
```

GitHub Actions' `perf.yml` tam olarak bunu yapar (`librocksdb-dev`'yi yükleyin, program bağdaştırıcılarını çalıştırın, çalıştırın
25 milyon satır matrisi, yapıyı yükleyin ve `backfill=true` olduğunda sayfayı doğrudan doldurun.

### kalibre

- **Veri/Önbellek Oranı**: 25.000.000 satır × 116 B ≈ 2766 MiB veri, motor başına 256 MiB önbellek,
  ≈ 11:1, RocksDB'nin 97 GiB veri/6 GiB önbelleğe (≈ 16:1) ilişkin resmi incelemesiyle uyumludur.
- **Aynı miktarda önbellek**: fkv'nin sayfa önbelleği (dizin/veri aynı havuz), RocksDB'nin blok önbelleği
  (`cache_index_and_filter_blocks=true`, blok boyutu 4 KiB), fjall'ın kendi blok önbelleği
  Bütçelerin tümü aynı sayıda bayta ayarlanmıştır.
- **Kalıcılık kalibresi (karşılaştırma sütunu, hizalama olmadan RocksDB varsayılan yapılandırmasını kullanır)**: fkv 1×veri sayfası + toplu iş başına
  1×manifest `fdatasync` (SyncData + AppendManifest of C++ EloqStore'a izomorfik,
  Her ikisinin de kapatma düğmesi yoktur); RocksDB sütunu **varsayılan yapılandırmayı** kullanır (WAL açık, `WriteOptions.sync=false`,
  Sıralı fsync yok), fjall ayrıca varsayılanı (`PersistMode::Buffer`) kullanır. Bu nedenle yazma aşaması (tek yazma /
  Toplu yazma işlemleri arasındaki fark, sayfada sayıyla birlikte belirtilmesi gereken uygulama farkı değil, **kalıcılık gücü farkıdır**.
  "Aynı senkronizasyon gücü altında kimin daha hızlı olduğunu" analiz etmek istiyorsanız `--align-fsync` ekleyin (RocksDB'yi sink=true ile açın,
  fjall SyncAll'ı kullanır), bu seçenek harici olarak yayınlanan matrisler için kullanılmaz.
- **İki fkv sürücüsü**: `--fkv-driver store` (varsayılan, süreç içinde doğrudan tek bir `Store`'ye bağlı ve
  RocksDB / fjall aynı zamanda kitaplık çağrılarıdır) ve `--fkv-driver runtime` (dağıtım modu: bir süreç)
  `--fkv-shards` parça iş parçacıkları, istekler anahtar tarafından yönlendirilir, C++ EloqStore'un genel okuma ve yazma işlemleriyle uyumlu hale getirilir
  arayüz). İkincisinin her isteğinin kuyruk + iş parçacığı arası uyandırma, yerel (macOS, dosya IO'su iş parçacığı havuzundan geçer) üzerinden geçmesi gerekir.
  Okuma süresi yaklaşık 20 µs/zamandır; bu, işlem içi doğrudan bağlantıdan çok daha yavaştır; Linux/io_uring'de çok daha ucuz, ancak
  Hala bir sıçrama. Sayfa matrisi mağaza kalibresini kullanır ve çalışma zamanı kalibresi "dağıtım formu vs C++" için kullanılır
  "Genel Arayüz" karşılaştırması (fkv çalışma zamanı tek iş parçacığı okuması yaklaşık 50 kops/s'dir, C++ EloqStore FFI tek iş parçacığı
  parça yaklaşık 21 kops/s).
- **Eşzamanlılık calibre'sini oku**: fkv yan iş parçacığı numarası = `min(readers, --read-max-concurrency)`, varsayılan
  Üst sınır 4'tür (değerlendirme makinesinin vCPU'su ile hizalanmıştır), "okuyucu başına bir iş parçacığı" **değildir** - aşılan düzey yalnızca kuyruğu artırır;
  İş parçacığı başına önbellek = 256 MiB / iş parçacığı sayısı (toplam miktar sabittir, okuyucularla parçalanmaz) ve kullanılır
  `FuturesUnordered` `--read-window` (varsayılan 16) istekleri anında saklayın. RocksDB tarafı
  `readers` Her `std::thread` aynı anda okur; fjall tarafında tek iş parçacıklı döngü (okuyucuların dişlilerini göz ardı ederek,
  Yalnızca referans sütunu olarak). `--no-fkv-read-threads`, "tek bir çalışma zamanında serpiştirilmiş N eşzamansız zincire" geri dönebilir.
  Bu, eşzamanlılık değil, boru hattı derinliğidir ve motorlar arasında karşılaştırılamaz.
- **len()**: Üç motorun hiçbirinde O(1) sayma arayüzü yoktur. Bu satır O(1) araması değil, tam tablo sayma oranıdır.