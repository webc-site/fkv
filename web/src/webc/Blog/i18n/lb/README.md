# Blog Post Komponent

- ** Automatesch Joer Gruppéierung **: Klassifizéieren automatesch Joer no Artikeldatum, an arrangéiert se an ëmgedréint Uerdnung no Joer
- ** Vereinfacht Datum Format **: Huelt automatesch de Joer Präfix beim Affichage, a weist nëmmen de Mount an den Dag
- ** Markdown Resumé an Text **: Built-in Lëscht Zesummefaassung Virschau an eenzegen Artikel komplett Text Rendering
- **Gradient dreidimensionalen Titel**: Integréieren `H1Txt` fir d'Iwwerschrëft an d'Verëffentlechungsdatum ze weisen

## Benotzt d'Demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["Titel", "abstrakt", "2026-07-27"]
};

// Wiesselt fir de spezifizéierte Artikelkierper ze lueden (passéiert eng eidel String fir zréck op d'Lëscht ze wiesselen)
blog.load("2026/publish");
</script>
```

## Interface Parameteren

### Eegeschaften a Methoden

- `blog.prefix`: Kritt oder setzt den Artikellink Präfix (wéi `"/blog/"`)
- `blog.mdUrl`: Kritt oder setzt d'Markdown Artikel Adress Generatioun Funktioun `(key, lang) => url`
- `blog._`: Kritt oder set den Artikellëscht Metadatenwörterbuch `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Wiesselt fir den Artikelinhalt vum spezifizéierte Schlëssel ze lueden, gitt eng eidel String `""` fir zréck op d'Bloglëscht ze wiesselen