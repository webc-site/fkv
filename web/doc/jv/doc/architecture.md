# Arsitektur

## runtime

Saben beling duwe thread OS eksklusif lan runtime compio. Panjaluk kasebut mlebu liwat antrian mlebu,
Diakoni dening mesin negara shard, rampung liwat panrimo oneshot.

## panyimpenan

Mesin nggunakake copy-on-write B + tree. kaca data, kaca indeks, chain kebanjiran lan
Manifes kasebut serialized menyang file segmen. cabang diidentifikasi dening `(branch, term)` lan dianakake saben
gambar nyata lan pemetaan.

## IO

`fkv_io` Nemtokake sipat IO asinkron. `fkv_io_compio` Dilaksanakake nganggo compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` nyedhiyakake memori lan backend injeksi fault, `fkv_object_s3` nyedhiyakake
S3 obyek backend.

## ketekunan

Manifes nyathet kaca root, root TTL, metadata cabang lan pemetaan. Nalika Mbukak maneh
Puter maneh manifest, banjur gawe maneh mapper kaca, banjur wiwiti panjaluk.