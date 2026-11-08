# Komponent tal-post tal-blog

- **Raggruppament tas-sena awtomatika**: Ikklassifika s-snin awtomatikament skont id-data tal-artikolu, u rranġahom f'ordni invers skont is-sena
- **Format ta' Data Simplifikat**: Neħħi awtomatikament il-prefiss tas-sena meta turi, u uri biss ix-xahar u l-jum
- **Sommarju u test tal-markdown**: Preview fil-qosor tal-lista integrata u għoti ta' test komplut ta' artikolu wieħed
- **Titolu tridimensjonali gradwali**: Integra `H1Txt` biex turi t-titlu u d-data tar-rilaxx

## Uża d-demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["titolu", "astratt", "2026-07-27"]
};

// Aqleb biex tgħabbi l-korp speċifikat ta’ l-artikolu (għaddi string vojta biex taqleb lura għal-lista)
blog.load("2026/publish");
</script>
```

## Parametri tal-interface

### Proprjetajiet u metodi

- `blog.prefix`: Ikseb jew issettja l-prefiss tal-link tal-artiklu (bħal `"/blog/"`)
- `blog.mdUrl`: Ikseb jew issettja l-funzjoni tal-ġenerazzjoni tal-indirizz tal-artikolu Markdown `(key, lang) => url`
- `blog._`: Ikseb jew issettja d-dizzjunarju tal-metadejta tal-lista tal-artikoli `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Aqleb biex tgħabbi l-kontenut tal-artiklu taċ-ċavetta speċifikata, għaddi string vojta `""` biex taqleb lura għal-lista tal-blogs