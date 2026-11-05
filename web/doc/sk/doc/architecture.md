# Architektúra

## runtime

Každý zlomok má exkluzívne vlákno operačného systému a runtime Compio. Požiadavka vstupuje cez vstupný front,
Vstup pomocou automatu na úlomok, dokončený prostredníctvom jednorazového potvrdenia.

## skladovanie

Motor používa copy-on-write B+strom. dátová stránka, indexová stránka, reťazec pretečenia a
Manifest je serializovaný do segmentových súborov. vetva je označená `(branch, term)` a je držaná každým
manifest a snímka mapovania.

## IO

`fkv_io` Definuje vlastnosť asynchrónneho IO. `fkv_io_compio` Implementované pomocou compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` poskytuje backendy na vkladanie pamäte a chýb, `fkv_object_s3` poskytuje
Backend objektu S3.

## vytrvalosť

Manifest zaznamenáva koreňovú stránku, koreň TTL, metadáta vetvy a mapovanie. Pri opätovnom otvorení
Najprv znova prehrajte manifest, potom znova vytvorte mapovač stránok a potom začnite poskytovať žiadosti.