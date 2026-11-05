# Címtárnavigációs komponens

- **Reszponzív fiók**: Megjeleníti az oldalsávot széles képernyőn, automatikusan összecsukja a keskeny képernyőt, és együttműködik a `c-side` fiók és a cím indítógombjával
- **Többnyelvű illesztőprogram**: Támogatja az objektumszótár vagy a többnyelvű funkciókat, automatikusan reagál a nyelvváltásra
- **Automatikus alkönyvtár**: A céltárolóban lévő `h1`~`h6` címek automatikus elemzése másodlagos könyvtár létrehozásához és a Hash nézetablak görgetési kiemelésének figyeléséhez.
- **Aszinkron betöltés**: Kattintson egy könyvtárelemre, hogy automatikusan aszinkron módon lekérje a Markdown szöveget, és beillessze a tartalom területére, megjelenítve `Ing` betöltési animációt

## Használd a demót

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
  "use": "összetevő használata"
});
</script>
```

## Interfész paraméterei

### Tulajdonságok és módszerek

- `toc.prefix`: Szerezze be vagy állítsa be az útválasztási előtag karakterláncát (például `"doc"`, a záró perjel nem kötelező), a `README` kulcs automatikusan egy üres elérési útra lesz leképezve (azaz magára az előtagra)
- `toc.li`: A címtár többnyelvű létrehozási funkciójának beszerzése vagy beállítása `(lang) => ({ key: title })`
- `toc.mdUrl`: Szerezze be vagy állítsa be a Markdown URL generálási funkciót `(key, lang) => url`
- `toc.target`: Szerezze be vagy állítsa be a társított célmegjelenítési csomópontot (például `c-md`). Ha nincs megadva, akkor automatikusan megkeresi a következőt: `c-md` / `main` / `article`
- `toc.load(key)`: A kulcsnak megfelelő dokumentumbetöltés és könyvtárbővítés kézi indítása
- `toc.sync()`: Manuálisan újraelemezze a címet az aktuális célcsomópontban, szinkronizálja a másodlagos könyvtárat és kiemeli az állapotot