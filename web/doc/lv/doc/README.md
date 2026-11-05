# fkv

fkv ir tīrs Rust iegults KV uzglabāšanas dzinējs.

## raksturīga

- Viens OS pavediens katrā shardā, kooperatīva darba cilpa.
- GOVS B+koks, pievienot tikai segmentu IO.
- lapa/indekss/manifests/pārpildes/saspiešanas formāts.
- lokālā/atmiņa/simulēta/S3 objekta aizmugursistēma.
- savrupais / mākoņa / gaidīšanas reprodukcijas režīms.
- Nav C ABI, nav Python SDK; tikai Rust ir publiskais interfeiss.

## Ieeja

- [Arhitektūra](architecture.md)
- [Crate](crates.md)
- [pārbaudi](testing.md)
- [sniegumu](performance.md)