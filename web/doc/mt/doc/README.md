# fkv

fkv hija magna tal-ħażna KV inkorporata Rust pura.

## karatteristika

- Ħajt OS wieħed għal kull shard, linja ta 'ħidma kooperattiva.
- COW B+siġra, segment IO ta' append-only.
- paġna/indiċi/manifest/overflow/format ta’ kompressjoni.
- backend tal-oġġett lokali/memorja/simulat/S3.
- modalità standalone/sħaba/replika standby.
- L-ebda C ABI, l-ebda Python SDK; Rust biss huwa l-interface pubbliku.

## Dħul

- [Arkitettura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [prestazzjoni](performance.md)