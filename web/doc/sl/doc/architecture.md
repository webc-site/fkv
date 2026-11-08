# Arhitektura

## čas izvajanja

Vsak delček ima ekskluzivno nit OS in izvajalno okolje Compio. Zahteva vstopi skozi vhodno čakalno vrsto,
Sprejem s strojem stanja delcev, izpolnjen s prejemom oneshot.

## shranjevanje

Mehanizem uporablja B+drevo kopiranja ob pisanju. podatkovna stran, indeksna stran, prelivna veriga in
Manifest je serializiran v segmentne datoteke. veja je označena z `(branch, term)` in jo ima vsak
manifest in posnetek preslikave.

## IO

`fkv_io` Definira lastnost asinhronega IO. `fkv_io_compio` Implementirano s compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` zagotavlja zaledja pomnilnika in vbrizgavanja napak, `fkv_object_s3` zagotavlja
Zaledje objekta S3.

## vztrajnost

Manifest beleži korensko stran, koren TTL, metapodatke veje in preslikavo. Ko ponovno odprete
Najprej ponovno predvajajte manifest, nato znova zgradite preslikavo strani in nato začnite streči zahteve.