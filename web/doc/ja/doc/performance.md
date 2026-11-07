# パフォーマンス

`fkv_bench` 固定キースペースで書き込み、読み取り、スキャンの負荷をテストします。

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

指標には、スループット、各ステージのレイテンシー、ピーク RSS が含まれます。リリース ワークフロー リリース x86_64 /
aarch64 の`fkv_bench`バイナリ。