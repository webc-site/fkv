# fkv

fkv on puhdas ruosteeseen upotettu KV-tallennusmoottori.

## ominaisuus

- Yksi käyttöjärjestelmäsäie per sirpale, yhteistyötyösilmukka.
- LEHMÄ B+puu, lisää vain segmentti IO.
- sivu/hakemisto/manifest/ylivuoto/pakkausmuoto.
- paikallinen/muisti/simuloitu/S3-objektin taustaohjelma.
- itsenäinen / pilvi / valmiustilakopiointitila.
- Ei C ABI:ta, ei Python SDK:ta; vain Rust on julkinen käyttöliittymä.

## Sisäänkäynti

- [Arkkitehtuuri](architecture.md)
- [Crate](crates.md)
- [testata](testing.md)
- [suorituskykyä](performance.md)