# Pensaernïaeth

## amser rhedeg

Mae gan bob shard edefyn OS unigryw ac amser rhedeg compio. Mae'r cais yn mynd trwy'r ciw dod i mewn,
Mynediad gan beiriant cyflwr shard, wedi'i gwblhau trwy dderbynneb un ergyd.

## storfa

Mae'r injan yn defnyddio coeden B+ copi-ar-ysgrifen. tudalen data, tudalen mynegai, cadwyn gorlif a
Mae'r maniffest yn cael ei gyfresoli i ffeiliau segment. cangen yn cael ei hadnabod gan `(branch, term)` ac yn cael ei dal gan bob un
maniffest a ciplun mapio.

## IO

`fkv_io` Yn diffinio'r nodwedd IO asyncronig. `fkv_io_compio` Wedi'i weithredu gyda compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` yn darparu backends pigiad cof a nam, `fkv_object_s3` yn darparu
S3 gwrthrych cefn.

## dyfalwch

Mae'r maniffest yn cofnodi tudalen gwraidd, gwraidd TTL, metadata cangen a mapio. Pan Ailagor
Ailchwaraewch y maniffest yn gyntaf, yna ailadeiladwch y mapiwr tudalen, ac yna dechreuwch weini ceisiadau.