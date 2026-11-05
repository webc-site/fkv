# Arxitektura

## ish vaqti

Har bir parchada eksklyuziv operatsion tizim va kompio ish vaqti mavjud. So'rov kirish navbati orqali kiradi,
Shard holati mashinasi orqali qabul qilish, bir martalik kvitansiya orqali yakunlandi.

## saqlash

Dvigatel B + daraxtidan nusxa ko'chirishdan foydalanadi. ma'lumotlar sahifasi, indeks sahifasi, to'ldirish zanjiri va
Manifest segment fayllarga ketma-ketlashtiriladi. filial `(branch, term)` tomonidan aniqlanadi va har birida saqlanadi
manifest va xaritalash surati.

## IO

`fkv_io` Asinxron IO xususiyatini belgilaydi. `fkv_io_compio` compio + io_uring bilan amalga oshirilgan,
`fkv_io_mem` / `fkv_io_sim` xotira va nosozliklarni in'ektsiya qilish uchun mo'ljallangan, `fkv_object_s3` ta'minlaydi
S3 ob'ektining orqa qismi.

## qat'iyatlilik

Manifestda ildiz sahifasi, TTL ildizi, filial metama'lumotlari va xaritalash qayd etiladi. Qayta ochilganda
Avval manifestni qayta o'ynang, so'ngra sahifa mapperini qayta yarating va so'ngra so'rovlarga xizmat ko'rsatishni boshlang.