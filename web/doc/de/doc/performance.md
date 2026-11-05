# Leistung

`fkv_bench` Testen Sie Schreib-, Lese- und Scan-Lasten auf einem festen Schlüsselbereich.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Zu den Indikatoren gehören Durchsatz, Latenz in jeder Phase und Spitzen-RSS. Release-Workflow Release x86_64 /
`fkv_bench` Binärdatei für aarch64.