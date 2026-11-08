# Memarlıq

## icra müddəti

Hər bir parçanın eksklüziv OS ipi və kompio işləmə vaxtı var. Sorğu giriş növbəsi ilə daxil olur,
Qəbul birdəfəlik qəbz vasitəsilə tamamlandı.

## saxlama

Mühərrik B+ağacından istifadə edir. məlumat səhifəsi, indeks səhifəsi, daşqın zənciri və
Manifest seqment fayllarına seriallaşdırılır. filial `(branch, term)` tərəfindən müəyyən edilir və hər biri tərəfindən tutulur
manifest və xəritəçəkmə snapshot.

## IO

`fkv_io` Asinxron IO xüsusiyyətini müəyyən edir. `fkv_io_compio` compio + io_uring ilə həyata keçirilir,
`fkv_io_mem` / `fkv_io_sim` yaddaş və xəta inyeksiya arxa uclarını təmin edir, `fkv_object_s3` təmin edir
S3 obyekt arxa sonu.

## əzmkarlıq

Manifest kök səhifəni, TTL kökünü, filial metadatasını və xəritələşdirməni qeyd edir. Yenidən açanda
Əvvəlcə manifesti təkrar oxuyun, sonra səhifə mapperini yenidən qurun və sonra sorğulara xidmət etməyə başlayın.