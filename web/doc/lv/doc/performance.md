# Veiktspēja

`fkv_bench` Pārbaudiet rakstīšanas, lasīšanas un skenēšanas ielādes fiksētā atslēgas vietā.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatori ietver caurlaidspēju, latentumu katrā posmā un maksimālo RSS. laidienu darbplūsmas izlaidums x86_64 /
`fkv_bench` binārs aarch64.