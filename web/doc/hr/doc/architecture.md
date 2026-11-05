# Arhitektura

## vrijeme izvođenja

Svaki shard ima ekskluzivnu OS nit i compio runtime. Zahtjev ulazi kroz ulazni red,
Pristup pomoću stroja stanja krhotina, izvršen putem oneshot potvrde.

## skladištenje

Motor koristi B+stablo kopiranja na pisanje. podatkovna stranica, indeksna stranica, preljevni lanac i
Manifest je serijaliziran u segmentne datoteke. grana je identificirana s `(branch, term)` i drži je svaka
manifest i snimak mapiranja.

## IO

`fkv_io` Definira značajku asinkronog IO-a. `fkv_io_compio` Implementirano s compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` pruža pozadinu memorije i ubacivanja grešaka, `fkv_object_s3` pruža
S3 pozadina objekta.

## upornost

Manifest bilježi korijensku stranicu, TTL korijen, metapodatke grane i mapiranje. Kada ponovno otvorite
Najprije ponovno reproducirajte manifest, zatim ponovno izgradite maper stranice, a zatim počnite posluživati ​​zahtjeve.