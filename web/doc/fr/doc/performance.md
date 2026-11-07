# Performances

`fkv_bench` Testez les charges d'écriture, de lecture et d'analyse sur un espace clé fixe.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Les indicateurs incluent le débit, la latence à chaque étape et le pic RSS. version du workflow version x86_64 /
`fkv_bench` binaire pour aarch64.