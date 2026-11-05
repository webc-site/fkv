# کارکردگی

`fkv_bench` ایک مقررہ کلیدی جگہ پر بوجھ لکھنے، پڑھنے اور اسکین کرنے کی جانچ کریں۔

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

اشارے میں تھرو پٹ، ہر مرحلے میں تاخیر، اور چوٹی RSS شامل ہیں۔ ریلیز ورک فلو ریلیز x86_64 /
`fkv_bench` بائنری برائے aarch64۔