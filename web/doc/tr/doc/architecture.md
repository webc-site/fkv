# Mimarlık

## çalışma zamanı

Her parçanın özel bir işletim sistemi iş parçacığı ve bir derleme çalışma zamanı vardır. İstek giriş kuyruğundan girer,
Parça durumu makinesiyle kabul, tek seferlik makbuzla tamamlanır.

## depolamak

Motor, yazarken kopyala B+ağacı kullanır. veri sayfası, dizin sayfası, taşma zinciri ve
Manifest, segment dosyaları halinde serileştirilir. dal `(branch, term)` ile tanımlanır ve her biri tarafından tutulur
manifest ve eşleme anlık görüntüsü.

## IO

`fkv_io` Eşzamansız GÇ özelliğini tanımlar. `fkv_io_compio` Compio + io_uring ile uygulandı,
`fkv_io_mem` / `fkv_io_sim` bellek ve hata ekleme arka uçlarını sağlar, `fkv_object_s3` sağlar
S3 nesne arka ucu.

## sebat

Bildirim, kök sayfayı, TTL kökünü, dal meta verilerini ve eşlemeyi kaydeder. Yeniden Açıldığında
Önce bildirimi yeniden oynatın, ardından sayfa eşleyiciyi yeniden oluşturun ve istekleri sunmaya başlayın.