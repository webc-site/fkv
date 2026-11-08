# Architektūra

## vykdymo laikas

Kiekviena dalis turi išskirtinę OS giją ir kompo vykdymo laiką. Užklausa patenka per įėjimo eilę,
Priėmimas šukės būsenos aparatu, baigtas naudojant oneshot kvitą.

## saugykla

Variklis naudoja kopijavimo ant rašymo B+ medį. duomenų puslapis, rodyklės puslapis, perpildymo grandinė ir
Aprašas suskirstytas į segmentų failus. filialas identifikuojamas `(branch, term)` ir yra kiekvienoje
manifestas ir žemėlapio momentinė nuotrauka.

## IO

`fkv_io` Apibrėžia asinchroninį IO požymį. `fkv_io_compio` Įdiegta naudojant compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` suteikia atminties ir gedimų įvedimo fonines programas, `fkv_object_s3` suteikia
S3 objekto užpakalinė programa.

## atkaklumas

Apraše įrašomi šakninis puslapis, TTL šaknis, šakos metaduomenys ir atvaizdavimas. Kai vėl atidaroma
Pirmiausia iš naujo paleiskite aprašą, tada iš naujo sukurkite puslapio atvaizdavimo priemonę ir pradėkite teikti užklausas.