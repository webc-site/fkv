# ڪارڪردگي

`fkv_bench` ٽيسٽ لکو، پڙهو، ۽ اسڪين لوڊ ڪريو مقرر ڪيل ڪي اسپيس تي.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

اشارن ۾ شامل آهن throughput، هر اسٽيج تي ويڪرائي، ۽ چوٽي RSS. ڪم فلو ڇڏڻ x86_64 /
`fkv_bench` بائنري aarch64 لاءِ.