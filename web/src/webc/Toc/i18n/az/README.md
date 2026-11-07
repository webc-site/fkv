# Kataloq Naviqasiya Komponenti

- **Cavablı siyirtmə**: Kenar panelini geniş ekranda göstərin, dar ekranda avtomatik yığışdırın və `c-side` siyirmə və başlıq tetik düyməsi ilə əməkdaşlıq edin
- **Çoxdilli sürücü**: Obyekt lüğətini və ya çoxdilli funksiyaları dəstəkləyir, dilin dəyişdirilməsinə avtomatik cavab verir
- **Avtomatik alt kataloq**: İkinci dərəcəli kataloq yaratmaq və Hash baxış pəncərəsinin sürüşdürülməsi vurğulanmasına nəzarət etmək üçün hədəf konteynerdəki `h1`~`h6` başlıqlarını avtomatik təhlil edin.
- **Asinxron yükləmə**: Markdown mətnini avtomatik olaraq asinxron əldə etmək və `Ing` yüklənmə animasiyasını göstərən məzmun sahəsinə yeritmək üçün kataloq elementinə klikləyin

## Demodan istifadə edin

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "istifadə": "komponentdən istifadə"
});
</script>
```

## İnterfeys parametrləri

### Xüsusiyyətlər və üsullar

- `toc.prefix`: Marşrut prefiks sətirini əldə edin və ya təyin edin (məsələn, `"doc"`, arxadakı slash isteğe bağlıdır), `README` düyməsi avtomatik olaraq boş yola (yəni, prefiksin özü) uyğunlaşdırılacaq.
- `toc.li`: Kataloqun çoxdilli yaradılması funksiyasını əldə edin və ya təyin edin `(lang) => ({ key: title })`
- `toc.mdUrl`: Markdown URL yaratma funksiyasını `(key, lang) => url` əldə edin və ya təyin edin
- `toc.target`: Əlaqəli hədəf göstərmə qovşağını əldə edin və ya təyin edin (məsələn, `c-md`). Göstərilməsə, o, avtomatik olaraq `c-md` / `main` / `article` tapacaq.
- `toc.load(key)`: Sənədin yüklənməsini və açara uyğun kataloqun genişləndirilməsini əl ilə işə salın
- `toc.sync()`: Cari hədəf qovşağında başlığı əl ilə yenidən təhlil edin, ikinci kataloqu sinxronlaşdırın və statusu vurğulayın