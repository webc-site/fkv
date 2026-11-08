# Bloq yazısı komponenti

- **Avtomatik il qruplaşdırılması**: İlləri məqalənin tarixinə görə avtomatik təsnif edin və onları il üzrə tərs qaydada təşkil edin
- **Sadələşdirilmiş Tarix Format**: Göstərərkən il prefiksini avtomatik silin və yalnız ay və günü göstərin
- **Markdown xülasəsi və mətn**: Daxili siyahı xülasəsi önizləməsi və tək məqalənin tam mətn göstərilməsi
- **Qradient üçölçülü başlıq**: Başlığı və buraxılış tarixini göstərmək üçün `H1Txt` inteqrasiya edin

## Demodan istifadə edin

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["başlıq", "abstrakt", "2026-07-27"]
};

// Göstərilən məqalənin gövdəsini yükləmək üçün keçin (siyahıya qayıtmaq üçün boş sətri keçin)
blog.load("2026/publish");
</script>
```

## İnterfeys parametrləri

### Xüsusiyyətlər və üsullar

- `blog.prefix`: Məqalə linki prefiksini əldə edin və ya təyin edin (məsələn, `"/blog/"`)
- `blog.mdUrl`: Markdown məqalə ünvanının yaradılması funksiyasını `(key, lang) => url` əldə edin və ya təyin edin
- `blog._`: `{ [key]: [title, summary, date] }` məqalə siyahısı metadata lüğətini əldə edin və ya təyin edin
- `blog.load(key)`: Göstərilən açarın məqalə məzmununu yükləmək üçün keçid edin, bloq siyahısına qayıtmaq üçün boş sətri `""` keçin