# Arkitektúr

## keyrslutími

Hvert brot hefur sérstakan OS þráð og compio keyrslutíma. Beiðnin fer í gegnum inngönguröðina,
Aðgangur með shard state vél, lokið með eintöku kvittun.

## geymsla

Vélin notar copy-on-write B+tré. gagnasíða, vísitölusíðu, yfirfallskeðja og
Upplýsingaskráin er sett í flokkaskrár. útibú er auðkennt með `(branch, term)` og er í vörslu hvers og eins
birtingarmynd og kortlagningarmynd.

## IO

`fkv_io` Skilgreinir ósamstilltan IO eiginleika. `fkv_io_compio` Útfært með compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` veitir minni og bilanasprautun bakenda, `fkv_object_s3` veitir
S3 hlutur bakendi.

## þrautseigju

Upplýsingaskráin skráir rótarsíðu, TTL rót, lýsigögn útibúa og kortlagningu. Þegar Opnað verður aftur
Spilaðu fyrst upplýsingaskrána aftur, endurbyggðu síðan síðukortara og byrjaðu síðan að birta beiðnir.