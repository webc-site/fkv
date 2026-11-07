# Architektura

## runtime

Každý fragment má exkluzivní vlákno operačního systému a běhový modul Compio. Požadavek vstupuje přes vstupní frontu,
Vstup pomocí střepového stavového automatu, dokončeno prostřednictvím jednorázového potvrzení.

## skladování

Motor používá kopírování při zápisu B+strom. datová stránka, indexová stránka, přetečovací řetězec a
Manifest je serializován do souborů segmentů. větev je označena `(branch, term)` a je držena každým
manifest a snímek mapování.

## IO

`fkv_io` Definuje vlastnost asynchronního IO. `fkv_io_compio` Implementováno pomocí compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` poskytuje backendy pro vkládání paměti a chyb, `fkv_object_s3` poskytuje
Backend objektu S3.

## perzistence

Manifest zaznamenává kořenovou stránku, kořen TTL, metadata větve a mapování. Když znovu otevřete
Nejprve znovu přehrajte manifest, poté znovu sestavte mapovač stránek a poté začněte poskytovat požadavky.