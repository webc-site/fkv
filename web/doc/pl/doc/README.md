# fkv

fkv to silnik pamięci masowej KV osadzony w czystym języku Rust.

## charakterystyczny

- Jeden wątek systemu operacyjnego na fragment, pętla współpracy.
- COW B+drzewo, segment I/O z możliwością tylko dołączenia.
- format strony/indeksu/manifestu/przepełnienia/kompresji.
- Zaplecze obiektu lokalnego/pamięci/symulowanego/S3.
- tryb repliki autonomicznej / w chmurze / w trybie gotowości.
- Żadnego C ABI, żadnego SDK Pythona; tylko Rust jest interfejsem publicznym.

## Wejście

- [Architektura](architecture.md)
- [Crate](crates.md)
- [test](testing.md)
- [wydajność](performance.md)