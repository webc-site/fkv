# Arhitektūra

## izpildlaiks

Katram fragmentam ir ekskluzīvs OS pavediens un kompozīciju izpildlaiks. Pieprasījums tiek ievadīts caur ieejas rindu,
Uzņemšana ar shard valsts iekārtu, kas pabeigta ar oneshot kvīti.

## uzglabāšana

Dzinējs izmanto kopēšanas uz rakstīšanas B+koku. datu lapa, rādītāja lapa, pārpildes ķēde un
Manifests tiek serializēts segmentu failos. filiāle tiek identificēta ar `(branch, term)` un tiek turēta katram
manifests un kartēšanas momentuzņēmums.

## IO

`fkv_io` Definē asinhrono IO pazīmi. `fkv_io_compio` Ieviests ar compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` nodrošina atmiņu un kļūdu ievadīšanas aizmugursistēmas, `fkv_object_s3` nodrošina
S3 objekta aizmugure.

## neatlaidība

Manifestā tiek ierakstīta saknes lapa, TTL sakne, filiāles metadati un kartēšana. Atkārtoti atverot
Vispirms atkārtoti atskaņojiet manifestu, pēc tam izveidojiet lapas kartētāju un pēc tam sāciet apkalpot pieprasījumus.