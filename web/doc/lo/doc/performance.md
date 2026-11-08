# ປະສິດທິພາບ

`fkv_bench` ທົດສອບການຂຽນ, ອ່ານ ແລະສະແກນການໂຫຼດໃນພື້ນທີ່ກະແຈຄົງທີ່.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

ຕົວຊີ້ວັດປະກອບມີການສົ່ງຜ່ານ, latency ໃນແຕ່ລະຂັ້ນຕອນ, ແລະ RSS ສູງສຸດ. ປ່ອຍ workflow release x86_64 /
`fkv_bench` ໄບນາຣີສຳລັບ aarch64.

## ການຈັບຄູ່ສາມທາງກັບ RocksDB/fjall

`crate/perf/fkv_compare` ໃຊ້ໄດເວີຂັ້ນຕອນດຽວກັນແລະການເຂົ້າລະຫັດລະຫັດດຽວກັນ (8-byte big-endian subscript ໃນຕອນທ້າຍ,
ຄໍາສັ່ງວັດຈະນານຸກົມແມ່ນສອດຄ່ອງກັບຄໍາສັ່ງຍ່ອຍ) ການທົດສອບ fkv, RocksDB, fjall: RocksDB ຜ່ານ
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp`ຫ້ອງສະໝຸດທີ່ແບ່ງປັນທີ່ຮວບຮວມຖືກໂຫຼດດ້ວຍ dlopen (ອັນດຽວກັນ C
ABI, Rust build ບໍ່ເຊື່ອມຕໍ່ກັບ RocksDB), fjall ໃຊ້ `fjall 3`ໂດຍກົງ. ແຕ່ລະສາມເຄື່ອງຈັກເປີດຫ້ອງສະຫມຸດ.
ກ້າວຫນ້າໃນຫ້ອງສະຫມຸດດຽວກັນໃນລໍາດັບຂອງຂັ້ນຕອນ: bulk load ສ້າງຂໍ້ມູນ, ແລະຫຼັງຈາກນັ້ນ len / point reading / range reading /
ການລຶບທັງໝົດມີຜົນຕໍ່ຂໍ້ມູນດຽວກັນ.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # ຂ້າມຖັນທີ 3 ຖ້າບໍ່ມີ RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

ຕົວເລກສຳລັບ matrix ຂອງໜ້າຫຼັກສາມາດມາຈາກການແລ່ນທີ່ສາມາດຕິດຕາມໄດ້ຄັ້ງດຽວເທົ່ານັ້ນ:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions' `perf.yml`ເຮັດແນວນີ້ແນ່ນອນ (ຕິດຕັ້ງ `librocksdb-dev`, ອະແດັບເຕີໂປຣແກຣມ, ແລ່ນ
25M row matrix, upload artifact, ແລະໂດຍກົງ backfill ຫນ້າໃນເວລາທີ່ `backfill=true`).

### calibre

- **Data/Cache Ratio**: 25,000,000 ແຖວ× 116 B ≈ 2766 MiB ຂອງຂໍ້ມູນ, 256 MiB ຂອງ cache ຕໍ່ເຄື່ອງຈັກ,
  ≈ 11:1, ສອດຄ່ອງກັບການທົບທວນຄືນຢ່າງເປັນທາງການຂອງ RocksDB ກ່ຽວກັບຂໍ້ມູນ 97 GiB/6 GiB cache (≈ 16:1).
- ** ຈໍາ​ນວນ​ທີ່​ດຽວ​ກັນ​ຂອງ​ຖານ​ຄວາມ​ຈໍາ **​: cache ຫນ້າ​ຂອງ fkv (ດັດ​ຊະ​ນີ / ຂໍ້​ມູນ​ສະ​ນຸກ​ເກີ​ດຽວ​ກັນ​)​, cache ຕັນ​ຂອງ RocksDB
  (`cache_index_and_filter_blocks=true`, ຂະໜາດບລັອກ 4 KiB), cache ບລັອກຂອງຕົນເອງຂອງ fjall
  ງົບປະມານທັງຫມົດແມ່ນຖືກກໍານົດເປັນຈໍານວນ bytes ດຽວກັນ.
- ** calibre ທົນນານ (ຖັນການປຽບທຽບໃຊ້ການຕັ້ງຄ່າເລີ່ມຕົ້ນຂອງ RocksDB, ໂດຍບໍ່ມີການຈັດຮຽງ)**: fkv 1×data page + per batch
  1× manifest `fdatasync` (Isomorphic to SyncData + AppendManifest ຂອງ C++ EloqStore,
  ບໍ່ມີສະຫຼັບປິດ); ຖັນ RocksDB ໃຊ້ ** ການຕັ້ງຄ່າເລີ່ມຕົ້ນ** (WAL ເປີດ, `WriteOptions.sync=false`,
  ບໍ່ມີ fsync ລໍາດັບ), fjall ຍັງໃຊ້ຄ່າເລີ່ມຕົ້ນ (`PersistMode::Buffer`). ດັ່ງນັ້ນໄລຍະການຂຽນ (ການຂຽນດຽວ /
  ຄວາມແຕກຕ່າງລະຫວ່າງການຂຽນ batch) ແມ່ນຄວາມແຕກຕ່າງກັນ ** ຄວາມທົນທານ **, ບໍ່ແມ່ນຄວາມແຕກຕ່າງຂອງການປະຕິບັດ, ເຊິ່ງຕ້ອງລະບຸໄວ້ໃນຫນ້າພ້ອມກັບຕົວເລກ.
  ຖ້າ​ຫາກ​ວ່າ​ທ່ານ​ຕ້ອງ​ການ​ວິ​ເຄາະ "ຜູ້​ທີ່​ໄວ​ຂຶ້ນ​ພາຍ​ໃຕ້​ຄວາມ​ເຂັ້ມ​ແຂງ synchronization ດຽວ​ກັນ​"​, ເພີ່ມ `--align-fsync` (ເປີດ RocksDB ກັບ sync=true​,
  fjall ໃຊ້ SyncAll), ທາງເລືອກນີ້ບໍ່ໄດ້ໃຊ້ສໍາລັບ matrices ທີ່ເຜີຍແຜ່ພາຍນອກ.
- **ສອງໄດເວີ fkv**: `--fkv-driver store` (ຄ່າເລີ່ມຕົ້ນ, ເຊື່ອມຕໍ່ໂດຍກົງກັບ `Store`ອັນດຽວພາຍໃນຂະບວນການ, ແລະ
  RocksDB / fjall ຍັງເປັນການເອີ້ນຫ້ອງສະໝຸດ) ແລະ `--fkv-driver runtime` (ຮູບແບບການນຳໃຊ້: ຂະບວນການ.
  `--fkv-shards`ກະທູ້ shard, ການຮ້ອງຂໍຖືກນໍາທາງໂດຍຄີ, ສອດຄ່ອງກັບການອ່ານສາທາລະນະແລະການຂຽນຂອງ C++ EloqStore
  ການໂຕ້ຕອບ). ການຮ້ອງຂໍແຕ່ລະອັນຕ້ອງຜ່ານຄິວ + ການປຸກຂ້າມກະທູ້, ແບບພື້ນເມືອງ (macOS, ໄຟລ໌ IO ໄປຜ່ານກຸ່ມກະທູ້)
  ເວລາອ່ານແມ່ນປະມານ 20 µs/ຄັ້ງ, ຊຶ່ງເປັນຄໍາສັ່ງຂອງຂະຫນາດຊ້າກວ່າການເຊື່ອມຕໍ່ໂດຍກົງໃນຂະບວນການ; ລາຄາຖືກກວ່າຫຼາຍໃນ Linux/io_uring, ແຕ່
  ຍັງໂດດ. ມາຕຣິກເບື້ອງໜ້າໃຊ້ caliber ຂອງຮ້ານ, ແລະ calibre runtime ຖືກນໍາໃຊ້ສໍາລັບ "deployment form vs C++
  ການປຽບທຽບ "ການໂຕ້ຕອບສາທາລະນະ" (fkv runtime single-thread reading is about 50 kops/s, C++ EloqStore FFI single-thread
  shard ປະມານ 21 kops/s).
- **ອ່ານ calibre concurrency**: fkv side thread number = `min(readers, --read-max-concurrency)`, ຄ່າເລີ່ມຕົ້ນ
  ຂອບເຂດຈໍາກັດເທິງແມ່ນ 4 (ສອດຄ່ອງກັບ vCPU ເຄື່ອງປະເມີນຜົນ), ** ບໍ່ ** "ຫນຶ່ງຫົວຂໍ້ຕໍ່ຜູ້ອ່ານ" - ລະດັບທີ່ເກີນພຽງແຕ່ເພີ່ມແຖວ;
  ແຄດຕໍ່ກະທູ້ = 256 MiB / ຈໍານວນຂອງກະທູ້ (ຈໍານວນທັງຫມົດແມ່ນຄົງ, ບໍ່ shredded ກັບຜູ້ອ່ານ), ແລະນໍາໃຊ້
  `FuturesUnordered`ຮັກສາ `--read-window` (ຄ່າເລີ່ມຕົ້ນ 16) ການຮ້ອງຂໍທັນທີ. ດ້ານ RocksDB ແມ່ນ
  `readers`ແຕ່ລະ `std::thread` ອ່ານພ້ອມໆກັນ; loop ເສັ້ນດຽວຢູ່ດ້ານ fjall (ບໍ່ສົນໃຈເຄື່ອງມືຜູ້ອ່ານ,
  ເປັນຖັນອ້າງອີງເທົ່ານັ້ນ). `--no-fkv-read-threads`ສາມາດ degenerate ກັບໄປ "interleaved N asynchronous chains ໃນ runtime ດຽວ".
  ນັ້ນແມ່ນຄວາມເລິກຂອງທໍ່, ບໍ່ແມ່ນຄວາມສອດຄ່ອງ, ແລະບໍ່ສາມາດປຽບທຽບໄດ້ໃນທົ່ວເຄື່ອງຈັກ.
- **len()**: ບໍ່ມີເຄື່ອງຈັກໃນສາມອັນໃດມີການໂຕ້ຕອບການນັບ O(1). ແຖວນີ້ແມ່ນອັດຕາການນັບຕາຕະລາງເຕັມ, ບໍ່ແມ່ນ O(1) lookup.