# Arhitektuur

## käitusaeg

Igal killul on eksklusiivne OS-i lõim ja kompio käitusaeg. Päring siseneb sissepääsujärjekorra kaudu,
Sissepääs shard state masinaga, lõpetatud oneshot kviitungiga.

## ladustamine

Mootor kasutab kopeerimis-kirjutamisel B+puud. andmeleht, registrileht, ülevooluahel ja
Manifest jadatakse segmendifailideks. haru tähistab `(branch, term)` ja seda hoiab igaüks
manifest ja kaardistamise hetktõmmis.

## IO

`fkv_io` Määrab asünkroonse IO tunnuse. `fkv_io_compio` Rakendatud koos compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` pakub mälu ja vea sisestamise taustaprogramme, `fkv_object_s3` pakub
S3 objekti taustaprogramm.

## püsivus

Manifest salvestab juurlehe, TTL juure, haru metaandmed ja vastenduse. Uuesti avamisel
Esmalt esitage manifest uuesti, seejärel koostage lehe kaardistaja ja seejärel alustage päringute esitamist.