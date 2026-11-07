# Našumas

`fkv_bench` Išbandykite rašymo, skaitymo ir nuskaitymo apkrovas fiksuotoje klavišų erdvėje.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Rodikliai apima pralaidumą, delsą kiekviename etape ir didžiausią RSS. išleidimo darbo eigos leidimas x86_64 /
`fkv_bench` dvejetainis, skirtas aarch64.