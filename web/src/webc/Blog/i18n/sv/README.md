# Blogginläggskomponent

- **Automatisk årsgruppering**: Klassificera automatiskt år enligt artikeldatum och ordna dem i omvänd ordning efter år
- **Förenklat datumformat**: Ta automatiskt bort årsprefixet vid visning och visa endast månad och dag
- **Markdown-sammanfattning och text**: Inbyggd förhandsgranskning av listsammanfattning och fullständig textåtergivning av en enda artikel
- **Gradient tredimensionell titel**: Integrera `H1Txt` för att visa rubrik och releasedatum

## Använd demon

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publicera": ["titel", "abstrakt", "2026-07-27"]
};

// Växla för att ladda den angivna artikelkroppen (passera en tom sträng för att växla tillbaka till listan)
blog.load("2026/publish");
</script>
```

## Gränssnittsparametrar

### Egenskaper och metoder

- `blog.prefix`: Hämta eller ställ in artikellänksprefixet (som `"/blog/"`)
- `blog.mdUrl`: Hämta eller ställ in Markdown-artikeladressgenereringsfunktionen `(key, lang) => url`
- `blog._`: Hämta eller ställ in artikellistans metadataordbok `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Växla för att ladda artikelinnehållet för den angivna nyckeln, skicka en tom sträng `""` för att växla tillbaka till blogglistan