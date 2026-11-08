# Markdown metnini güvenli bir şekilde ayrıştırın ve HTML'ye dönüştürün.

- **Güvenlik Temizleme**: Yerel `setHTML` API veya `DOMParser` temizleyicileri kullanarak XSS saldırılarına karşı savunma yapın
- **İçerik Çıkarma**: Başlatma sırasında varsayılan yuvanın metin içeriğini otomatik olarak okuyun ve oluşturun
- **Yerleşik dizgi**: önceden ayarlanmış liste, tablo, kod bloğu, alıntı ve uyarı kutusu stilleri

## Demoyu kullanın

### statik oluşturma

```html
<c-md>
# başlık
Bu bir Markdown içeriğidir
</c-md>
```

### dinamik atama

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Yeni başlık\nYeni içerik";
</script>
```

## Arayüz parametreleri

Markdown metnini doğrudan etiketin içine aktarın (varsayılan yuva olarak) veya `value` aracılığıyla dinamik olarak güncelleyin.

## Stil açıklaması

`.Md` stil sınıfı, bileşen başlatıldığında otomatik olarak eklenir. Aşağıdaki Markdown genişletilmiş uyarı kutusu düzenini destekler (referans sözdizimiyle tetiklenir):

- `[!NOTE]`: Bilgi istemi bilgisi
- `[!TIP]`: Öneriler/İpuçları
- `[!IMPORTANT]`: Önemli dikkat
- `[!WARNING]`: Risk uyarısı
- `[!CAUTION]`: Son derece yüksek risk uyarısı

## güvenlik mekanizması

Oluşturma sırasında HTML'yi güvenli bir şekilde enjekte etmek için önce yerel `setHTML` API'sini çağırın. Tarayıcı bunu desteklemiyorsa otomatik olarak `DOMParser` tabanlı özyinelemeli temizleyiciye geçiş yapacak ve aşağıdaki güvenlik politikasını uygulayacaktır:
- Güvenli olmayan etiketleri kaldırın (ör. `script`, `iframe` vb.)
- `on*` etkinlik özelliklerini temizle
- `javascript:`, `vbscript:` ve `data:` (`data:image/` hariç) gibi tehlikeli protokollerle başlayan öznitelikleri filtreleyerek XSS saldırılarına karşı savunma yapın.