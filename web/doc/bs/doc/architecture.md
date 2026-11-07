# Arhitektura

## runtime

Svaki dio ima ekskluzivnu OS nit i vrijeme izvođenja compio. Zahtjev ulazi kroz ulazni red,
Prijem pomoću državnog stroja fragmenata, završen jednokratnim prijemom.

## skladištenje

Motor koristi B+stablo kopiranja na upisivanje. stranica podataka, stranica indeksa, lanac prelijevanja i
Manifest je serijalizovan u segmentne fajlove. grana je identificirana sa `(branch, term)` i svaka je drži
manifest i snimak mapiranja.

## IO

`fkv_io` Definira asinhroni IO svojstvo. `fkv_io_compio` Implementirano sa compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` obezbjeđuje memoriju i pozadinsko ubacivanje grešaka, `fkv_object_s3` pruža
Pozadina S3 objekta.

## upornost

Manifest bilježi korijensku stranicu, TTL korijen, metapodatke grane i mapiranje. Kada se ponovo otvori
Prvo reproducirajte manifest, zatim ponovo napravite maper stranice, a zatim počnite posluživati ​​zahtjeve.