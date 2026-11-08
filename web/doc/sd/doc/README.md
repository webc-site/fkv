# fkv

fkv هڪ خالص زنگ لڳل KV اسٽوريج انجڻ آهي.

## خاصيت

- هڪ او ايس ٿريڊ في شارڊ، ڪوآپريٽو ڪم لوپ.
- COW B+ وڻ، ضم ڪرڻ وارو صرف حصو IO.
- صفحو/انڊيڪس/منظور/اوور فلو/کمپريشن فارميٽ.
- local/memory/simulated/S3 اعتراض پس منظر.
- standalone / cloud / standby replica mode.
- نه C ABI، نه Python SDK؛ صرف زنگ عوامي انٽرفيس آهي.

## داخلا

- [آرڪيٽيڪچر](architecture.md)
- [Crate](crates.md)
- [ٽيسٽ](testing.md)
- [ڪارڪردگي](performance.md)