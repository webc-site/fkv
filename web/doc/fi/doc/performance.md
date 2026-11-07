# Suorituskyky

`fkv_bench` Testaa kirjoitus-, luku- ja skannauskuormia kiinteällä näppäinalueella.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikaattorit sisältävät suorituskyvyn, latenssin kussakin vaiheessa ja huippu-RSS:n. vapauta työnkulku julkaisu x86_64 /
`fkv_bench` binääri aarch64:lle.