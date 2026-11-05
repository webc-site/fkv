# Arkitekturë

## koha e ekzekutimit

Çdo copëz ka një fillesë ekskluzive të OS dhe një kohë ekzekutimi kompio. Kërkesa hyn përmes radhës së hyrjes,
Pranimi me makineri shtetërore të copëtuar, i plotësuar me faturën oneshot.

## magazinimit

Motori përdor kopjimin në shkrim B+peme. faqja e të dhënave, faqja e indeksit, zinxhiri i tejmbushjes dhe
Manifesti është i serializuar në skedarë segmenti. dega identifikohet nga `(branch, term)` dhe mbahet nga secili
manifestimi dhe fotografia e hartës.

## IO

`fkv_io` Përcakton tiparin IO asinkron. `fkv_io_compio` Zbatuar me compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` ofron memorie dhe injektimin e gabimeve, `fkv_object_s3` ofron
Mbështetja e objektit S3.

## këmbëngulje

Manifesti regjistron faqen rrënjë, rrënjën TTL, meta të dhënat e degës dhe hartën. Kur Rihapet
Fillimisht riluajeni manifestin, më pas rindërtoni hartuesin e faqeve dhe më pas filloni të shërbeni kërkesat.