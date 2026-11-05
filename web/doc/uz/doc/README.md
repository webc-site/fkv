# fkv

fkv - bu sof Rust o'rnatilgan KV saqlash mexanizmi.

## xarakterli

- Har bir parcha uchun bitta OS ishi, kooperativ ish davri.
- COW B+daraxt, faqat qo‘shiladigan IO segmenti.
- sahifa/indeks/manifest/toshib ketish/siqish formati.
- mahalliy/xotira/simulyatsiyalangan/S3 ob'ektining orqa tomoni.
- mustaqil / bulutli / kutish replika rejimi.
- C ABI yo'q, Python SDK yo'q; faqat Rust umumiy interfeysdir.

## Kirish

- [Arxitektura](architecture.md)
- [Crate](crates.md)
- [sinov](testing.md)
- [ishlash](performance.md)