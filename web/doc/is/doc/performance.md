# Afköst

`fkv_bench` Prófaðu að skrifa, lesa og skanna álag á föstu lyklarými.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Vísar innihalda afköst, leynd á hverju stigi og hámarks RSS. gefa út verkflæðisútgáfu x86_64 /
`fkv_bench` tvöfaldur fyrir aarch64.