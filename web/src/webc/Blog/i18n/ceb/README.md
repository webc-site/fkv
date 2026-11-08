# Ang bahin sa post sa blog

- **Awtomatikong paggrupo sa tuig**: Awtomatikong pagklasipikar ang mga tuig sumala sa petsa sa artikulo, ug ihan-ay kini sa baliktad nga han-ay sa tuig
- **Gisimple nga Format sa Petsa**: Awtomatikong tangtangon ang prefix sa tuig kung ipakita, ug ipakita ra ang bulan ug adlaw
- **Markdown summary ug text**: Built-in list summary preview ug single article complete text rendering
- **Gradient three-dimensional nga titulo**: I-integrate ang `H1Txt` para ipakita ang headline ug petsa sa pagpagawas

## Gamita ang demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["titulo", "abstract", "2026-07-27"]
};

// Pagbalhin aron makarga ang gipiho nga lawas sa artikulo (ipasa ang usa ka walay sulod nga pisi aron ibalik sa lista)
blog.load("2026/publish");
</script>
```

## Mga parametro sa interface

### Mga kabtangan ug pamaagi

- `blog.prefix`: Kuhaa o itakda ang prefix sa link sa artikulo (sama sa `"/blog/"`)
- `blog.mdUrl`: Kuhaa o i-set ang Markdown nga artikulo sa paghimo og adres function `(key, lang) => url`
- `blog._`: Kuhaa o itakda ang metadata nga diksyonaryo sa lista sa artikulo `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Pagbalhin aron i-load ang artikulo nga sulod sa gipiho nga yawe, ipasa ang walay sulod nga string `""` aron mobalik sa listahan sa blog