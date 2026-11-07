# Wydajność

`fkv_bench` Testuj obciążenia zapisu, odczytu i skanowania w stałej przestrzeni kluczy.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Wskaźniki obejmują przepustowość, opóźnienie na każdym etapie i szczyt RSS. wydanie przepływu pracy wydanie x86_64 /
`fkv_bench` plik binarny dla aarch64.