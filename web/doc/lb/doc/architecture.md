# Architektur

## runtime

All Shard huet en exklusiven OS Fuedem an eng Compio Runtime. D'Ufro geet duerch d'Ingressschlaang,
Entrée vun Shard Staat Maschinn, fäerdeg duerch Oneshot Empfang.

## Stockage

De Motor benotzt Kopie-op-schreiwen B + Bam. daten Säit, Index Säit, Iwwerschwemmung Kette an
De Manifest gëtt a Segmentdateien serialiséiert. Branche gëtt duerch `(branch, term)` identifizéiert a gëtt vun all gehal
Manifestatioun a Kartéierungssnapshot.

## IO

`fkv_io` Definéiert den asynchronen IO Trait. `fkv_io_compio` Implementéiert mat compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` bitt Erënnerung a Feelerinjektioun Backends, `fkv_object_s3` bitt
S3 Objet Backend.

## Ausdauer

De Manifest registréiert Root Säit, TTL Root, Filial Metadaten a Kartéierung. Wann Reouverture
Fir d'éischt de Manifest ze spillen, da baut de Säitmapper op, a fänkt dann un Ufroen ze servéieren.