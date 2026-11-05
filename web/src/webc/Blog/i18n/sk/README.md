# Komponent blogového príspevku

- **Automatické zoskupovanie rokov**: Automaticky klasifikujte roky podľa dátumu článku a usporiadajte ich v opačnom poradí podľa roku
- **Zjednodušený formát dátumu**: Automaticky odstrániť predponu roka pri zobrazení a zobraziť iba mesiac a deň
- **Zhrnutie a text označenia**: Zabudovaná ukážka súhrnu zoznamu a vykreslenie úplného textu jedného článku
- **Trojrozmerný názov s prechodom**: Integrujte `H1Txt` na zobrazenie nadpisu a dátumu vydania

## Použite demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["title", "abstract", "2026-07-27"]
};

// Prepnite na načítanie špecifikovaného tela článku (prepnite späť na zoznam zadaním prázdneho reťazca)
blog.load("2026/publish");
</script>
```

## Parametre rozhrania

### Vlastnosti a metódy

- `blog.prefix`: Získajte alebo nastavte predponu odkazu na článok (napríklad `"/blog/"`)
- `blog.mdUrl`: Získajte alebo nastavte funkciu generovania adresy článku Markdown `(key, lang) => url`
- `blog._`: Získajte alebo nastavte slovník metadát zoznamu článkov `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Prepnúť na načítanie obsahu článku zadaného kľúča, zadaním prázdneho reťazca `""` prepnúť späť na zoznam blogov