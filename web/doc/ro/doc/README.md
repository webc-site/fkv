# fkv

fkv este un motor de stocare KV încorporat pur Rust.

## caracteristică

- Un fir de operare pentru fiecare fragment, buclă de lucru cooperativă.
- COW B+tree, segment IO numai pentru atașare.
- format de pagină/index/manifest/overflow/compresie.
- backend obiect local/memorie/simulat/S3.
- modul de replică standalone / cloud / standby.
- Fără C ABI, fără Python SDK; doar Rust este interfața publică.

## Intrare

- [Arhitectură](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [performanţă](performance.md)