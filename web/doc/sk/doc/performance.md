# Výkon

`fkv_bench` Otestujte zaťaženie zapisovania, čítania a skenovania na pevnom kľúčovom priestore.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Medzi indikátory patrí priepustnosť, latencia v každej fáze a vrchol RSS. vydanie pracovného postupu vydanie x86_64 /
`fkv_bench` binárne pre aarch64.