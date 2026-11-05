# Blogipostituse komponent

- **Automaatne aastate rühmitamine**: klassifitseerige aastad automaatselt artikli kuupäeva järgi ja järjestage need aastate kaupa vastupidises järjekorras
- **Lihtsustatud kuupäevavorming**: eemaldage kuvamisel automaatselt aasta eesliide ja kuvage ainult kuu ja päev
- **Märgistuste kokkuvõte ja tekst**: sisseehitatud loendi kokkuvõtte eelvaade ja ühe artikli täieliku teksti renderdamine
- **Gradient kolmemõõtmeline pealkiri**: integreerige `H1Txt`, et kuvada pealkiri ja väljalaskekuupäev

## Kasutage demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["pealkiri", "abstract", "2026-07-27"]
};

// Lülitage määratud artikli keha laadimiseks (loendisse tagasi lülitumiseks edastage tühi string)
blog.load("2026/publish");
</script>
```

## Liidese parameetrid

### Omadused ja meetodid

- `blog.prefix`: hankige või määrake artikli lingi eesliide (nt `"/blog/"`)
- `blog.mdUrl`: hankige või määrake Markdowni artikli aadressi genereerimise funktsioon `(key, lang) => url`
- `blog._`: hankige või määrake artiklite loendi metaandmete sõnastik `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Lülitage määratud võtme artikli sisu laadimiseks, sisestage tühi string `""`, et lülituda tagasi blogiloendisse