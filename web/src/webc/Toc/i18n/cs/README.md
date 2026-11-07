# Komponenta navigace v adresáři

- **Responzivní zásuvka**: Zobrazte postranní panel na širokoúhlé obrazovce, automaticky se sbalí na úzké obrazovce a spolupracuje s tlačítkem `c-side` zásuvky a titulku
- **Vícejazyčný ovladač**: Podporuje objektový slovník nebo vícejazyčné funkce, automaticky reaguje na přepínání jazyků
- **Automatický podadresář**: Automaticky analyzovat `h1`~`h6` tituly v cílovém kontejneru za účelem vygenerování sekundárního adresáře a sledování zvýraznění posouvání výřezu hash.
- **Asynchronní načítání**: Kliknutím na položku adresáře automaticky asynchronně získáte text Markdown a vložíte jej do oblasti obsahu, přičemž se zobrazí animace načítání `Ing`

## Použijte demo

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "použití komponenty"
});
</script>
```

## Parametry rozhraní

### Vlastnosti a metody

- `toc.prefix`: Získejte nebo nastavte řetězec předpony směrování (např. `"doc"`, koncové lomítko je volitelné), klíč `README` bude automaticky namapován na prázdnou cestu (tj. samotnou předponu)
- `toc.li`: Získejte nebo nastavte funkci vícejazyčného generování adresáře `(lang) => ({ key: title })`
- `toc.mdUrl`: Získejte nebo nastavte funkci generování adresy URL Markdown `(key, lang) => url`
- `toc.target`: Získejte nebo nastavte přidružený cílový vykreslovací uzel (například `c-md`). Pokud nezadáte, automaticky vyhledá `c-md` / `main` / `article`
- `toc.load(key)`: Ruční spuštění načítání dokumentu a rozšíření adresáře podle klíče
- `toc.sync()`: Ručně znovu analyzujte název v aktuálním cílovém uzlu, synchronizujte sekundární adresář a stav zvýraznění