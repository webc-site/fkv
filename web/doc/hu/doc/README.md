# fkv

Az fkv egy tiszta Rust beágyazott KV tárolómotor.

## jellegzetes

- Egy operációs rendszer szál szilánkonként, együttműködő munkahurok.
- TEHÉN B+fa, csak az IO szegmens hozzáfűzése.
- oldal/index/manifest/túlcsordulás/tömörítési formátum.
- helyi/memória/szimulált/S3 objektum háttérrendszer.
- önálló / felhő / készenléti replika mód.
- Nincs C ABI, nincs Python SDK; csak a Rust a nyilvános felület.

## Bejárat

- [Építészet](architecture.md)
- [Crate](crates.md)
- [teszt](testing.md)
- [teljesítmény](performance.md)