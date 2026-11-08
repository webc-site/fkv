# Markdown mətnini HTML-də təhlükəsiz şəkildə təhlil edin və göstərin.

- **Təhlükəsizlik Sanitasiyası**: Doğma `setHTML` API və ya `DOMParser` təmizləyicilərdən istifadə edərək XSS hücumlarına qarşı müdafiə edin
- **Məzmun çıxarılması**: Başlanğıc zamanı defolt yuvanın mətn məzmununu avtomatik oxuyun və onu göstərin
- **Daxili yazılama**: əvvəlcədən təyin edilmiş siyahı, cədvəl, kod bloku, sitat və xəbərdarlıq qutusu üslubları

## Demodan istifadə edin

### statik göstərmə

```html
<c-md>
# başlıq
Bu Markdown məzmununun bir hissəsidir
</c-md>
```

### dinamik tapşırıq

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Yeni başlıq\nYeni məzmun";
</script>
```

## İnterfeys parametrləri

Markdown mətnini birbaşa etiketin içərisinə daxil edin (default slot olaraq), və ya `value` vasitəsilə dinamik olaraq yeniləyin.

## Stil təsviri

Komponent işə salındıqda, `.Md` stil sinfi avtomatik olaraq əlavə edilir. Aşağıdakı Markdown genişləndirilmiş xəbərdarlıq qutusu tərtibatını dəstəkləyir (istinad sintaksisi ilə işə salınır):

- `[!NOTE]`: Tez məlumat
- `[!TIP]`: Təkliflər/Tövsiyələr
- `[!IMPORTANT]`: Vacib diqqət
- `[!WARNING]`: Risk xəbərdarlığı
- `[!CAUTION]`: Çox yüksək risk xəbərdarlığı

## təhlükəsizlik mexanizmi

Təqdim edərkən, HTML-ni təhlükəsiz yeritmək üçün ilk olaraq yerli `setHTML` API-yə zəng edin. Brauzer onu dəstəkləmirsə, o, avtomatik olaraq `DOMParser` əsasında rekursiv təmizləyiciyə endirəcək və aşağıdakı təhlükəsizlik siyasətini həyata keçirəcək:
- Təhlükəsiz teqləri silin (məsələn, `script`, `iframe` və s.)
- `on*` hadisə atributlarını silin
- `javascript:`, `vbscript:` və `data:` (`data:image/` istisna olmaqla) kimi təhlükəli protokollarla başlayan atributları süzərək XSS hücumlarından müdafiə olun.