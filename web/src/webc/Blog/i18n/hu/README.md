# Blogbejegyzés komponens

- **Automatikus évcsoportosítás**: Az évek automatikus osztályozása a cikk dátuma szerint, és fordított sorrendbe rendezése évenként
- **Egyszerűsített dátumformátum**: Az év előtag automatikus eltávolítása megjelenítéskor, és csak a hónap és a nap megjelenítése
- **Jelölések összefoglalása és szöveg**: Beépített lista-összefoglaló előnézet és egyetlen cikk teljes szövegének megjelenítése
- **Gradiens háromdimenziós cím**: Integrálja az `H1Txt` elemet a cím és a megjelenési dátum megjelenítéséhez

## Használd a demót

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/közzététel": ["cím", "absztrakt", "2026-07-27"]
};

// Váltás a megadott cikktörzs betöltéséhez (adjon át egy üres karakterláncot, hogy visszaváltson a listára)
blog.load("2026/publish");
</script>
```

## Interfész paraméterei

### Tulajdonságok és módszerek

- `blog.prefix`: A cikk hivatkozási előtagjának beszerzése vagy beállítása (például `"/blog/"`)
- `blog.mdUrl`: Szerezze be vagy állítsa be a Markdown cikkcímgeneráló funkciót `(key, lang) => url`
- `blog._`: A cikklista metaadatszótárának lekérése vagy beállítása `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Váltás a megadott kulcs cikktartalmának betöltéséhez, adjon át egy üres karakterláncot `""` a bloglistára való visszatéréshez