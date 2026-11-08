# Arkitektur

## køretid

Hvert shard har en eksklusiv OS-tråd og en compio-runtime. Anmodningen kommer ind gennem indgangskøen,
Adgang med shard state-maskine, afsluttet med oneshot-kvittering.

## opbevaring

Motoren bruger copy-on-write B+tree. dataside, indeksside, overløbskæde og
Manifestet serialiseres til segmentfiler. filial er identificeret med `(branch, term)` og ejes af hver
snapshot af manifest og kortlægning.

## IO

`fkv_io` Definerer det asynkrone IO-træk. `fkv_io_compio` Implementeret med compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` giver hukommelse og fejlinjektion backends, `fkv_object_s3` giver
S3 objekt backend.

## udholdenhed

Manifestet registrerer rodside, TTL-rod, grenmetadata og kortlægning. Når genåbner
Afspil først manifestet igen, genopbyg derefter sidemapperen, og begynd derefter at vise anmodninger.