# Arkitektura

## exekuzioa

Zati bakoitzak OS hari esklusibo bat eta compio exekuzio-denbora ditu. Eskaera sarrera-ilaratik sartzen da,
Onarpena shard egoera makinaren bidez, oneshot ordainagiria bidez osatuta.

## biltegiratzea

Motorrak kopia-idazketa B+zuhaitza erabiltzen du. datu orria, aurkibide orria, gainezka-katea eta
Manifestua segmentu fitxategietan serializatuta dago. adarra `(branch, term)` bidez identifikatzen da eta bakoitzak bere esku dauka
manifestua eta maparen argazkia.

## IO

`fkv_io` IO ezaugarri asinkronoa definitzen du. `fkv_io_compio` Compio + io_uring-ekin inplementatuta,
`fkv_io_mem` / `fkv_io_sim`k memoria eta akatsen injekzio backendak eskaintzen ditu, `fkv_object_s3`k eskaintzen ditu
S3 objektuaren backend-a.

## iraunkortasuna

Manifestuak erro orria, TTL erroa, adar metadatuak eta mapak erregistratzen ditu. Berriro irekitzean
Lehenik eta behin, errepikatu manifestua, gero berreraiki orri-mapatzailea eta, gero, hasi eskaerak hornitzen.