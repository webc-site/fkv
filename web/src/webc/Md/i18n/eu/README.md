# Analizatu eta errendatu Markdown testua HTMLra modu seguruan.

- **Segurtasun garbiketa**: defendatu XSS erasoen aurka `setHTML` API edo `DOMParser` garbitzaile natiboak erabiliz
- **Edukia erauztea**: lehenetsitako zirrikituaren testu-edukia automatikoki irakurri hasieratzerakoan eta errendatu
- **Konposaketa integratua**: aurrez ezarritako zerrenda, taula, kode-blokea, komatxo eta abisu-koadroaren estiloak

## Erabili demoa

### errendatze estatikoa

```html
<c-md>
# izenburua
Markdown edukiaren zati bat da
</c-md>
```

### esleipen dinamikoa

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Izenburu berria\nEduki berria";
</script>
```

## Interfazearen parametroak

Idatzi Markdown testua zuzenean etiketaren barruan (zirrikitu lehenetsi gisa), edo eguneratu dinamikoki `value` bidez.

## Estiloaren deskribapena

`.Md` estilo-klasea automatikoki gehitzen da osagaia hasieratzen denean. Markdown hedatutako abisu-koadroaren diseinua onartzen du (erreferentzia sintaxiaren arabera abiarazia):

- `[!NOTE]`：Informazio bizkorra
- `[!TIP]`: Iradokizunak/Aholkuak
- `[!IMPORTANT]`: arreta garrantzitsua
- `[!WARNING]`: Arriskuaren abisua
- `[!CAUTION]`: arrisku oso altuko abisua

## segurtasun mekanismoa

Errendatzean, deitu lehenik `setHTML` jatorrizko APIra HTML modu seguruan sartzeko. Arakatzaileak ez badu onartzen, `DOMParser`n oinarritutako garbitzaile errekurtsibo batera jaitsiko da automatikoki eta segurtasun-politika hau ezarriko du:
- Kendu seguruak ez diren etiketak (adibidez, `script`, `iframe`, etab.)
- Garbitu `on*` gertaeren atributuak
- Defendatu XSS erasoen aurka, `javascript:`, `vbscript:` eta `data:` bezalako protokolo arriskutsuekin hasten diren atributuak iragaziz (`data:image/` izan ezik).