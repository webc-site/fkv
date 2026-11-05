# Blogaren argitalpenaren osagaia

- **Urteen multzokatze automatikoa**: automatikoki sailkatu urteak artikuluaren dataren arabera, eta antolatu alderantzizko ordenan urteka.
- **Data formatu sinplifikatua**: automatikoki kendu urtearen aurrizkia bistaratzen denean, eta soilik bistaratu hilabetea eta eguna.
- **Markdown laburpena eta testua**: zerrendaren laburpenaren aurrebista eta artikulu bakarreko testu osoa errendatzea
- **Gradientearen hiru dimentsioko izenburua**: integratu `H1Txt` izenburua eta kaleratze data bistaratzeko

## Erabili demoa

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["izenburua", "laburpena", "2026-07-27"]
};

// Aldatu zehaztutako artikuluaren gorputza kargatzeko (pasatu kate huts bat zerrendara itzultzeko)
blog.load("2026/publish");
</script>
```

## Interfazearen parametroak

### Propietateak eta metodoak

- `blog.prefix`: lortu edo ezarri artikuluaren estekaren aurrizkia (adibidez, `"/blog/"`)
- `blog.mdUrl`: Lortu edo ezarri Markdown artikuluaren helbidea sortzeko funtzioa `(key, lang) => url`
- `blog._`: Lortu edo ezarri artikulu-zerrendako metadatuen hiztegia `{ [key]: [title, summary, date] }`
- `blog.load(key)`: aldatu zehaztutako gakoaren artikuluaren edukia kargatzeko, pasatu kate huts bat `""` blog-zerrendara itzultzeko