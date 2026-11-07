# Arkitektura

## runtime

Ang bawat shard ay may eksklusibong OS thread at compio runtime. Ang kahilingan ay pumapasok sa pamamagitan ng pagpasok na pila,
Pagpasok sa pamamagitan ng shard state machine, nakumpleto sa pamamagitan ng oneshot na resibo.

## imbakan

Gumagamit ang makina ng copy-on-write na B+tree. data page, index page, overflow chain at
Ang manifest ay naka-serialize sa mga segment na file. ang sangay ay kinilala ng `(branch, term)` at hawak ng bawat isa
manifest at mapping snapshot.

## IO

`fkv_io` Tinutukoy ang asynchronous na katangian ng IO. `fkv_io_compio` Ipinatupad gamit ang compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` ay nagbibigay ng memory at fault injection backend, `fkv_object_s3` ay nagbibigay
S3 object backend.

## pagpupursige

Itinala ng manifest ang root page, TTL root, branch metadata at mapping. Nang Muling Buksan
I-replay muna ang manifest, pagkatapos ay muling buuin ang page mapper, at pagkatapos ay simulan ang paghahatid ng mga kahilingan.