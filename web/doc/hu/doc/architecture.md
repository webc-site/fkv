# Építészet

## futásidő

Minden shardnak van egy exkluzív operációs rendszer-szála és egy kompo futtatókörnyezete. A kérés a belépési soron keresztül érkezik,
Belépés szilánkos állapotú géppel, oneshot nyugtával teljesítve.

## tárolás

A motor másolás-írásra B+fát használ. adatlap, index oldal, túlcsordulási lánc és
A jegyzék szegmensfájlokká szerializálva van. az ágat a `(branch, term)` azonosítja, és mindegyik birtokolja
manifeszt és leképezési pillanatkép.

## IO

`fkv_io` Meghatározza az aszinkron IO tulajdonságot. `fkv_io_compio` Megvalósítva a compio + io_uring,
A `fkv_io_mem` / `fkv_io_sim` memóriát és hibabefecskendezési háttérprogramokat biztosít, a `fkv_object_s3` pedig
S3 objektum háttérrendszer.

## kitartás

A jegyzék rögzíti a gyökéroldalt, a TTL-gyökeret, az ág metaadatait és a leképezést. Amikor újra megnyílik
Először játssza le újra a jegyzéket, majd építse újra az oldalleképezőt, majd kezdje meg a kérések kiszolgálását.