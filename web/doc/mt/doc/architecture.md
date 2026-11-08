# Arkitettura

## runtime

Kull shard għandu ħajt OS esklussiv u runtime compio. It-talba tidħol mill-kju tad-dħul,
Ammissjoni permezz ta 'magna tal-istat shard, mimlija permezz ta' irċevuta oneshot.

## ħażna

Il-magna tuża copy-on-write B + tree. paġna tad-dejta, paġna tal-indiċi, katina tal-overflow u
Il-manifest huwa serialized f'fajls tas-segmenti. fergħa hija identifikata minn `(branch, term)` u hija miżmuma minn kull wieħed
manifest u snapshot tal-mapping.

## IO

`fkv_io` Jiddefinixxi l-karatteristika IO asinkronika. `fkv_io_compio` Implimentat b'compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` jipprovdi memorja u backends ta’ injezzjoni tal-ħsara, `fkv_object_s3` jipprovdi
S3 oġġett backend.

## persistenza

Il-manifest jirreġistra l-paġna tal-għerq, l-għerq TTL, il-metadejta tal-fergħa u l-immappjar. Meta Terġa' tiftaħ
L-ewwel erġa' rrepeti l-manifest, imbagħad ibni mill-ġdid il-mapper tal-paġna, u mbagħad ibda jaqdi t-talbiet.