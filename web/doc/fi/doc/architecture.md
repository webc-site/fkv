# Arkkitehtuuri

## suoritusaika

Jokaisella sirpaleella on yksinomainen käyttöjärjestelmäsäie ja compion ajonaika. Pyyntö saapuu sisääntulojonon kautta,
Sisäänpääsy shard state -koneella, suoritettu oneshot-kuitilla.

## varastointi

Moottori käyttää kopiointi-kirjoitus B+-puuta. tietosivu, hakemistosivu, ylivuotoketju ja
Luettelo on sarjoitettu segmenttitiedostoiksi. haara on tunnistettu `(branch, term)`:lla ja jokaisella on se
manifesti ja kartoitusvedos.

## IO

`fkv_io` Määrittää asynkronisen IO-ominaisuuden. `fkv_io_compio` Toteutettu compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` tarjoaa muistin ja vikojen lisäyksen taustaohjelmat, `fkv_object_s3` tarjoaa
S3 objektin taustaohjelma.

## sinnikkyys

Luettelo tallentaa juurisivun, TTL-juuren, haaran metatiedot ja kartoituksen. Kun avataan uudelleen
Toista ensin luettelo uudelleen, muodosta sitten sivukartoitus uudelleen ja aloita sitten pyyntöjen näyttäminen.