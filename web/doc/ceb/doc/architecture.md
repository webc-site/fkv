# Arkitektura

## runtime

Ang matag shard adunay usa ka eksklusibo nga OS thread ug usa ka compio runtime. Ang hangyo mosulod pinaagi sa ingress queue,
Admission pinaagi sa shard state machine, nahuman pinaagi sa oneshot nga resibo.

## pagtipig

Ang makina naggamit ug copy-on-write B+ tree. data page, index page, overflow chain ug
Ang manifest gi-serialize sa mga file nga bahin. sanga giila sa `(branch, term)` ug gihuptan sa matag usa
manifest ug mapping snapshot.

## IO

`fkv_io` Naghubit sa asynchronous nga kinaiya sa IO. `fkv_io_compio` Gipatuman gamit ang compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` naghatag og memory ug fault injection backends, `fkv_object_s3` naghatag
S3 object backend.

## pagpadayon

Ang manifest nagrekord sa gamut nga panid, TTL gamut, metadata sa sanga ug mapping. Pag-abli pag-usab
Una i-replay ang manifest, dayon tukuron pag-usab ang page mapper, ug dayon sugdi ang pagserbisyo sa mga hangyo.