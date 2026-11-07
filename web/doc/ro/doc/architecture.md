# Arhitectură

## timpul de rulare

Fiecare fragment are un fir de operare exclusiv și un timp de execuție compio. Solicitarea intră prin coada de intrare,
Admiterea prin mașina de stare shard, finalizată prin chitanță oneshot.

## depozitare

Motorul folosește arborele B+copy-on-write. pagină de date, pagină de index, lanț de overflow și
Manifestul este serializat în fișiere segment. ramura este identificată prin `(branch, term)` și este deținută de fiecare
manifest și instantaneu de cartografiere.

## IO

`fkv_io` Definește trăsătura IO asincronă. `fkv_io_compio` Implementat cu compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` oferă memorie și backend-uri de injectare a erorilor, `fkv_object_s3` oferă
S3 obiect backend.

## persistenţă

Manifestul înregistrează pagina rădăcină, rădăcina TTL, metadatele ramurilor și maparea. Când se redeschid
Mai întâi redați manifestul, apoi reconstruiți mapatorul de pagini și apoi începeți să difuzați cereri.