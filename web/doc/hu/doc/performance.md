# Teljesítmény

`fkv_bench` Tesztelje az írási, olvasási és szkennelési terheléseket egy rögzített kulcsterületen.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

A mutatók közé tartozik az átviteli sebesség, a késleltetés az egyes szakaszokban és a csúcs RSS. kiadás munkafolyamat kiadás x86_64 /
`fkv_bench` bináris az aarch64 számára.