# Arkitektur

## kjøretid

Hvert shard har en eksklusiv OS-tråd og en compio-kjøretid. Forespørselen kommer inn gjennom ingresskøen,
Inngang med shard state-maskin, fullført gjennom oneshot-kvittering.

## lagring

Motoren bruker copy-on-write B+tree. dataside, indeksside, overløpskjede og
Manifestet er serialisert til segmentfiler. gren er identifisert med `(branch, term)` og holdes av hver
øyeblikksbilde av manifest og kartlegging.

## IO

`fkv_io` Definerer den asynkrone IO-egenskapen. `fkv_io_compio` Implementert med compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` gir minne og feilinjeksjonsbackends, `fkv_object_s3` gir
S3-objekt bakenden.

## standhaftighet

Manifestet registrerer rotside, TTL-rot, grenmetadata og kartlegging. Når gjenåpner
Spill først manifestet på nytt, bygg deretter sidekartleggingen på nytt, og begynn deretter å levere forespørsler.