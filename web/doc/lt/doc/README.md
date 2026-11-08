# fkv

fkv yra grynas Rust įterptas KV saugojimo variklis.

## charakteristika

- Viena OS gija vienai skeveldrai, bendra darbo kilpa.
- KARVE B+medis, pridėti tik segmentą IO.
- puslapis/indeksas/manifestas/perpildymas/glaudinimo formatas.
- vietinis / atmintis / imituotas / S3 objekto užpakalinė programa.
- autonominis / debesies / budėjimo režimas.
- Nėra C ABI, nėra Python SDK; tik Rust yra vieša sąsaja.

## Įėjimas

- [Architektūra](architecture.md)
- [Crate](crates.md)
- [bandymas](testing.md)
- [pasirodymas](performance.md)