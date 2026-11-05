# Arkitektur

## körning

Varje skärva har en exklusiv OS-tråd och en compio-körtid. Förfrågan kommer in genom ingångskön,
Inträde med shard state-maskin, slutförd genom oneshot-kvitto.

## lagring

Motorn använder copy-on-write B+tree. datasida, indexsida, overflow chain och
Manifestet serialiseras till segmentfiler. gren identifieras av `(branch, term)` och innehas av var och en
ögonblicksbild av manifest och kartläggning.

## IO

`fkv_io` Definierar den asynkrona IO-egenskapen. `fkv_io_compio` Implementerad med compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` ger minne och felinjicering backends, `fkv_object_s3` ger
S3 objekt backend.

## uthållighet

Manifestet registrerar rotsida, TTL-rot, grenmetadata och mappning. När öppnas igen
Spela först upp manifestet igen, bygg sedan om sidmapparen och börja sedan visa förfrågningar.