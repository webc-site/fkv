# Performanca

`fkv_bench` Testoni ngarkesat e shkrimit, leximit dhe skanimit në një hapësirë ​​fikse të çelësit.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Treguesit përfshijnë xhiron, vonesën në çdo fazë dhe kulmin e RSS. lëshimi i rrjedhës së punës lëshimi x86_64 /
`fkv_bench` binar për aarch64.