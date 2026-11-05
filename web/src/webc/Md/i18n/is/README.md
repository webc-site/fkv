# Þjálfa og skila Markdown texta á öruggan hátt í HTML.

- **Öryggishreinsun**: Verjast XSS árásum með því að nota innfædd `setHTML` API eða `DOMParser` hreinsiefni
- **Efnisútdráttur**: Lestu sjálfkrafa textainnihald sjálfgefna raufarinnar meðan á frumstillingu stendur og skilaðu því
- **Innbyggð tegundastilling**: forstilltur listi, tafla, kóðablokk, tilvitnun og viðvörunarkassa stíll

## Notaðu kynninguna

### truflanir flutningur

```html
<c-md>
# titill
Þetta er hluti af Markdown efni
</c-md>
```

### kraftmikið verkefni

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nýr titill\nNýtt efni";
</script>
```

## Viðmótsbreytur

Sendu Markdown-texta beint inn í merkið (sem sjálfgefinn rauf), eða uppfærðu á virkan hátt með `value`.

## Stíllýsing

`.Md` stílflokknum er sjálfkrafa bætt við þegar íhluturinn er frumstilltur. Styður eftirfarandi útfærslu Markdown viðvörunarkassa (kveikt af tilvísunarsetningafræði):

- `[!NOTE]`：Hvetjandi upplýsingar
- `[!TIP]`: Tillögur/Ábendingar
- `[!IMPORTANT]`: Mikilvæg athygli
- `[!WARNING]`: Hættuviðvörun
- `[!CAUTION]`: Viðvörun um mjög mikla áhættu

## öryggiskerfi

Hringdu fyrst í innfædda `setHTML` API til að sprauta HTML á öruggan hátt. Ef vafrinn styður það ekki mun hann sjálfkrafa niðurfæra í endurkvæmt hreinsiefni byggt á `DOMParser` og innleiða eftirfarandi öryggisstefnu:
- Fjarlægðu óörugg merki (t.d. `script`, `iframe` osfrv.)
- Hreinsaðu `on*` atburðareiginleika
- Verjast XSS árásum með því að sía eiginleika sem byrja með hættulegum samskiptareglum eins og `javascript:`, `vbscript:` og `data:` (nema `data:image/`).