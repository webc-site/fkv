# Unumdorlik

`fkv_bench` Ruxsat etilgan kalit maydonida yuklarni yozish, o'qish va skanerlashni sinab ko'ring.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Ko'rsatkichlar o'tkazish qobiliyati, har bir bosqichda kechikish va eng yuqori RSSni o'z ichiga oladi. ish oqimini chiqarish x86_64 /
`fkv_bench` aarch64 uchun ikkilik.

## RocksDB/fjall bilan uch tomonlama moslashuv

`crate/perf/fkv_compare` Xuddi shu bosqich drayveridan va bir xil kalit kodlashdan foydalaning (oxirida 8 baytli big-endian subscript,
Lug'at tartibi pastki belgilar tartibiga mos keladi) Test fkv, RocksDB, fjall: RocksDB o'tdi
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Kompilyatsiya qilingan umumiy kutubxona dlopen bilan yuklangan (xuddi shu C
ABI, Rust qurilishi RocksDB bilan bog'lanmaydi), fjall to'g'ridan-to'g'ri `fjall 3` dan foydalanadi. Uch dvigatelning har biri kutubxonani ochadi.
Bosqichlar bo'yicha bir xil kutubxonada oldinga siljish: ommaviy yuk ma'lumotlarni yaratadi, keyin esa len / nuqta o'qish / diapazonni o'qish /
O'chirishlarning barchasi bir xil ma'lumotlarga ta'sir qiladi.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # RocksDB bo'lmasa, uchinchi ustunni o'tkazib yuboring
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Bosh sahifa matritsasi uchun raqamlar faqat bitta kuzatilishi mumkin bo'lgan ishga tushishi mumkin:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions' `perf.yml` aynan shunday qiladi (`librocksdb-dev` o'rnating, dastur adapterlarini o'rnating, ishga tushiring
25M qatorli matritsa, artefaktni yuklang va `backfill=true` boʻlganda sahifani toʻgʻridan-toʻgʻri toʻldiring.

### kalibrli

- **Maʼlumotlar/Kesh nisbati**: 25.000.000 qator × 116 B ≈ 2766 Mb maʼlumot, har bir dvigatel uchun 256 Mb kesh,
  ≈ 11:1, RocksDB tomonidan 97 GiB maʼlumot/6 GiB kesh (≈ 16:1) boʻyicha rasmiy koʻrib chiqishga mos keladi.
- **Bir xil miqdordagi kesh**: fkv sahifa keshi (indeks/ma’lumotlar bir xil hovuz), RocksDB blok keshi
  (`cache_index_and_filter_blocks=true`, blok hajmi 4 KiB), fjallning o'z blok keshi
  Byudjetlarning barchasi bir xil bayt soniga o'rnatiladi.
- **Doimiylik kalibri (taqqoslash ustuni RocksDB standart konfiguratsiyasidan moslamasiz foydalanadi)**: fkv 1×ma’lumotlar sahifasi + to‘plam uchun
  1×manifest `fdatasync` (SyncData ga izomorf + C++ EloqStore ilovasining AppendManifest,
  Ikkalasida ham o'chirish tugmasi yo'q); RocksDB ustuni **standart konfiguratsiyadan** foydalanadi (WAL on, `WriteOptions.sync=false`,
  Ketma-ket fsync yo'q), fjall sukut bo'yicha ham foydalanadi (`PersistMode::Buffer`). Shuning uchun yozish bosqichi (bitta yozish /
  To'plamdagi yozishlar orasidagi farq) bu raqam bilan birga sahifada ko'rsatilishi kerak bo'lgan amalga oshirish farqi emas, **qat'iylik kuchi farqi**.
  Agar siz "bir xil sinxronizatsiya kuchida kim tezroq ekanligini" tahlil qilmoqchi bo'lsangiz, `--align-fsync` qo'shing (RocksDB-ni sync=true bilan oching,
  fjall SyncAll-dan foydalanadi), bu parametr tashqi nashr etilgan matritsalar uchun ishlatilmaydi.
- **Ikki fkv drayveri**: `--fkv-driver store` (standart, jarayon davomida bevosita bitta `Store`ga ulangan va
  RocksDB / fjall ham kutubxona chaqiruvlari) va `--fkv-driver runtime` (tartibga solish rejimi: jarayon
  `--fkv-shards` parchalar, so'rovlar C++ EloqStore-ni ommaviy o'qish va yozish bilan moslashtirilgan kalit bo'yicha yo'naltiriladi
  interfeysi). Ikkinchisining har bir so'rovi navbatdan o'tishi kerak + o'zaro ish zarralarini uyg'otish, mahalliy (macOS, IO fayli iplar hovuzidan o'tadi)
  O'qish vaqti taxminan 20 mks/vaqtni tashkil etadi, bu jarayondagi to'g'ridan-to'g'ri ulanishga qaraganda sekinroq; Linux/io_uringda ancha arzon, lekin
  Hali ham sakrash. Sahifa matritsasi do'kon kalibridan foydalanadi va ish vaqti kalibri "tarqatish shakli va C++" uchun ishlatiladi
  "Ommaviy interfeys" ni solishtirish (fkv ish vaqtining bir ipli o'qishi taxminan 50 kops/s, C++ EloqStore FFI bir-torli.
  shard taxminan 21 kops/s).
- **Bir vaqtning o‘zida o‘qish kalibri**: fkv yon ip raqami = `min(readers, --read-max-concurrency)`, birlamchi
  Yuqori chegara 4 (baholash mashinasi vCPU bilan mos), ** emas** "har bir o'quvchi uchun bitta ip" - oshib ketgan daraja faqat navbatni oshiradi;
  Har bir mavzu uchun kesh = 256 Mb / iplar soni (umumiy miqdor aniqlangan, o'quvchilar bilan parchalanmagan) va ishlatilgan
  `FuturesUnordered` `--read-window` (standart 16) soʻrovlarni tezda saqlang. RocksDB tomoni
  `readers` Har bir `std::thread` bir vaqtning o'zida o'qiydi; Fjall tomonidagi bitta tishli halqa (o'quvchilarning tishli vositalariga e'tibor bermasdan,
  Faqat mos yozuvlar ustuni sifatida). `--no-fkv-read-threads` "bitta ish vaqtida interleaved N asinxron zanjir" ga qaytishi mumkin.
  Bu quvur liniyasining chuqurligi, bir vaqtning o'zida emas va dvigatellar bilan taqqoslanmaydi.
- **len()**: Uch dvigatelning hech birida O(1) hisoblash interfeysi mavjud emas. Bu qator O (1) qidiruvi emas, balki to'liq jadval hisoblash tezligidir.