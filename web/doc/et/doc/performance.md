# Jõudlus

`fkv_bench` Testige kirjutamise, lugemise ja skannimise koormusi fikseeritud võtmeruumil.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Näitajad hõlmavad läbilaskevõimet, latentsust igas etapis ja tipp-RSS-i. vabasta töövoo väljalase x86_64 /
`fkv_bench` binaarfail aarch64 jaoks.