# Errendimendua

`fkv_bench` Probatu idazketa, irakurketa eta eskaneatu kargak gako-espazio finko batean.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Adierazleen artean fluxua, latentzia etapa bakoitzean eta RSS gailurra daude. askatu lan-fluxuaren oharra x86_64 /
`fkv_bench` aarch64rako bitarra.