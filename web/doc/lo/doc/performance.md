# ປະສິດທິພາບ

`fkv_bench` ທົດສອບການຂຽນ, ອ່ານ ແລະສະແກນການໂຫຼດໃນພື້ນທີ່ກະແຈຄົງທີ່.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

ຕົວຊີ້ວັດປະກອບມີການສົ່ງຜ່ານ, latency ໃນແຕ່ລະຂັ້ນຕອນ, ແລະ RSS ສູງສຸດ. ປ່ອຍ workflow release x86_64 /
`fkv_bench` ໄບນາຣີສຳລັບ aarch64.