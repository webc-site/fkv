# Okuma deneyimi için optimize edilmiş tipografi stilleri

- **Temel dizgi**: Başlıklara, paragraflara, kalın, italik, üstü çizili, satır içi koda, resimlere ve bölme çizgilerine uyum sağlayın.
- **Bağlantı animasyonu**: Fareyle üzerine gelindiğinde alt çizgi animasyonunu görüntüleyin.
- **Proje listesi**: Sırasız listeler madde işaretlerini, sıralı listeler ise yuvarlatılmış arka plan seri numaralarını kullanır ve çok düzeyli iç içe yerleştirmeyi destekler.
- **Görev Listesi**: Onay kutusu stili sağlar, yapılacak ve tamamlandı durumlarını destekler.
- **Uyarı İpuçları**: Beş tür uyarı kutusunu destekler: Not, İpucu, Önemli, Uyarı ve Dikkat.
- **Tablolar ve Kod**: Tablo satırının üzerine gelindiğinde vurgulama ve gölgeler ve degrade arka planlar içeren kod blokları sağlar.

## Demoyu kullanın

```html
<div class="Md">
  <h1>başlık</h1>
  <p>Paragraf metni ve <a href="#">bağlantı</a></p>
  <hr>
  <h2>İkinci düzey başlık</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Görev Öğesi</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Not içeriği</p>
  </blockquote>
</div>
```

## stil sınıfı

Stiller, `.Md` kapsayıcısı altındaki HTML öğelerine uygulanır.

### Uyarı kutusu stili

Uyarı kutusunu değiştirmek için `blockquote` öğesine `.q` sınıf adı ile aşağıdaki sınıf adını ekleyin:

- `note`: Notlar
- `tip`: İpucu
- `important`: Önemli
- `warning`: uyarı
- `caution`: Not