# fkv

fkv Rust txertatutako KV biltegiratze motor hutsa da.

## ezaugarria

- OS hari bat zati bakoitzeko, lan kooperatiboaren begizta.
- COW B+zuhaitza, eranskin-soilik segmentua IO.
- orria/aurkibidea/manifestazioa/gainezkatzea/konpresioaren formatua.
- lokala/memoria/simulatua/S3 objektuaren backend-a.
- autonomoa / hodeia / egonean erreplika modua.
- Ez C ABI, ez Python SDK; Rust bakarrik interfaze publikoa da.

## Sarrera

- [Arkitektura](architecture.md)
- [Crate](crates.md)
- [proba](testing.md)
- [errendimendua](performance.md)