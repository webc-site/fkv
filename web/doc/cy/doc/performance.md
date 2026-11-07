# Perfformiad

`fkv_bench` Profwch ysgrifennu, darllen, a sganiwch lwythi ar fwlch bysell sefydlog.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Mae'r dangosyddion yn cynnwys trwygyrch, hwyrni ar bob cam, a RSS brig. rhyddhau llif gwaith rhyddhau x86_64 /
`fkv_bench` deuaidd ar gyfer aarch64.