# Komponenta příspěvku na blogu

- **Automatické seskupování roků**: Automaticky klasifikujte roky podle data článku a uspořádejte je v opačném pořadí podle roku
- **Zjednodušený formát data**: Při zobrazení automaticky odstraní předponu roku a zobrazí pouze měsíc a den
- **Shrnutí a text označení**: Vestavěný náhled souhrnu seznamu a vykreslování celého textu jednoho článku
- **Trojrozměrný název s přechodem**: Integrací `H1Txt` zobrazíte titulek a datum vydání

## Použijte demo

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

// Přepnutím se načte zadané tělo článku (předáním prázdného řetězce přepnete zpět na seznam)
blog.load("2026/publish");
</script>
```

## Parametry rozhraní

### Vlastnosti a metody

- `blog.prefix`: Získejte nebo nastavte předponu odkazu na článek (například `"/blog/"`)
- `blog.mdUrl`: Získejte nebo nastavte funkci generování adresy článku Markdown `(key, lang) => url`
- `blog._`: Získejte nebo nastavte slovník metadat seznamu článků `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Přepnutím se načte obsah článku zadaného klíče, předáním prázdného řetězce `""` přepnete zpět na seznam blogů