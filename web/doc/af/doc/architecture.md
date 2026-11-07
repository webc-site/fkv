# Argitektuur

## looptyd

Elke skerf het 'n eksklusiewe OS-draad en 'n compio-looptyd. Die versoek gaan deur die ingangstou,
Toelating per shard staat masjien, voltooi deur een shot kwitansie.

## berging

Die enjin gebruik kopieer-op-skryf B+boom. data bladsy, indeks bladsy, oorloop ketting en
Die manifes word in segmentlêers gerangskik. tak word geïdentifiseer deur `(branch, term)` en word deur elkeen gehou
manifesteer en kartering momentopname.

## IO

`fkv_io` Definieer die asynchrone IO-eienskap. `fkv_io_compio` Geïmplementeer met compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` verskaf geheue en foutinspuiting-agtergronde, `fkv_object_s3` verskaf
S3 voorwerp agterkant.

## volharding

Die manifes teken wortelbladsy, TTL-wortel, takmetadata en kartering aan. Wanneer Heropen
Speel eers die manifes weer, herbou dan die bladsykartering en begin dan om versoeke te bedien.