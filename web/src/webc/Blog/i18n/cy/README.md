# Elfen post blog

- **Grŵp blwyddyn awtomatig**: Dosbarthu blynyddoedd yn awtomatig yn ôl dyddiad yr erthygl, a'u trefnu yn ôl y flwyddyn
- **Fformat Dyddiad Syml**: Tynnwch y rhagddodiad blwyddyn yn awtomatig wrth arddangos, ac arddangoswch y mis a'r diwrnod yn unig
- **Crynodeb Markdown a thestun**: Rhagolwg cryno o'r rhestr wedi'i gynnwys a rendrad testun cyflawn erthygl sengl
- **Teitl tri dimensiwn graddiant**: Integreiddiwch `H1Txt` i ddangos y pennawd a'r dyddiad rhyddhau

## Defnyddiwch y demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/cyhoeddi": ["teitl", "abstract", "2026-07-27"]
};

// Newid i lwytho'r corff erthygl penodedig (pasiwch linyn gwag i newid yn ôl i'r rhestr)
blog.load("2026/publish");
</script>
```

## Paramedrau rhyngwyneb

### Priodweddau a dulliau

- `blog.prefix`: Cael neu osod rhagddodiad dolen yr erthygl (fel `"/blog/"`)
- `blog.mdUrl`: Cael neu osod swyddogaeth creu cyfeiriad erthygl Markdown `(key, lang) => url`
- `blog._`: Cael neu osod geiriadur metadata rhestr erthyglau `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Newidiwch i lwytho cynnwys erthygl y bysell benodedig, pasiwch linyn gwag `""` i newid yn ôl i'r rhestr blogiau