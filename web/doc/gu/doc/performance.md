# કામગીરી

`fkv_bench` નિયત કી સ્પેસ પર લોડ લખવા, વાંચવા અને સ્કેન કરવાનું પરીક્ષણ કરો.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

સૂચકાંકોમાં થ્રુપુટ, દરેક તબક્કે લેટન્સી અને પીક RSSનો સમાવેશ થાય છે. વર્કફ્લો રિલીઝ x86_64 /
aarch64 માટે `fkv_bench` બાઈનરી.