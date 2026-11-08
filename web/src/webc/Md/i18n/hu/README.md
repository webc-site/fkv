# A Markdown szöveg biztonságos elemzése és renderelése HTML-be.

- **Biztonsági fertőtlenítés**: Védelem az XSS-támadások ellen natív `setHTML` API vagy `DOMParser` tisztító segítségével
- **Tartalom kinyerése**: Automatikusan beolvassa az alapértelmezett hely szöveges tartalmát az inicializálás során, és megjeleníti azt
- **Beépített betűszedés**: előre beállított lista, táblázat, kódblokk, idézet és figyelmeztető doboz stílusok

## Használd a demót

### statikus megjelenítés

```html
<c-md>
# cím
Ez a Markdown tartalom egy darabja
</c-md>
```

### dinamikus hozzárendelés

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Új cím\nÚj tartalom";
</script>
```

## Interfész paraméterei

Adja meg a Markdown szöveget közvetlenül a címkén belül (alapértelmezett helyként), vagy frissítse dinamikusan a `value` segítségével.

## Stílusleírás

A `.Md` stílusosztály automatikusan hozzáadódik az összetevő inicializálásakor. Támogatja a következő Markdown kiterjesztett figyelmeztető doboz elrendezést (a hivatkozás szintaxisa váltja ki):

- `[!NOTE]`:Sürgős információ
- `[!TIP]`: Javaslatok/Tippek
- `[!IMPORTANT]`: Fontos figyelem
- `[!WARNING]`: Kockázati figyelmeztetés
- `[!CAUTION]`: Rendkívül magas kockázatú figyelmeztetés

## biztonsági mechanizmus

Rendereléskor először hívja meg a natív `setHTML` API-t a HTML biztonságos beillesztéséhez. Ha a böngésző nem támogatja, akkor automatikusan visszavált a `DOMParser` alapján rekurzív tisztítóra, és végrehajtja a következő biztonsági házirendet:
- Távolítsa el a nem biztonságos címkéket (pl. `script`, `iframe` stb.)
- Törölje az `on*` eseményattribútumokat
- Az XSS-támadások elleni védekezés veszélyes protokollokkal kezdődő attribútumok szűrésével, például `javascript:`, `vbscript:` és `data:` (kivéve `data:image/`).