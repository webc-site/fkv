# Bezpečně analyzujte a vykreslujte text Markdown do HTML.

- **Bezpečnostní dezinfekce**: Chraňte se před XSS útoky pomocí nativních `setHTML` API nebo `DOMParser` čističů
- **Extrakce obsahu**: Automaticky načte textový obsah výchozího slotu během inicializace a vykreslí jej
- **Vestavěná sazba**: styly přednastaveného seznamu, tabulky, bloku kódu, nabídky a upozornění

## Použijte demo

### statické vykreslování

```html
<c-md>
# titul
Toto je část obsahu Markdown
</c-md>
```

### dynamické přiřazení

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nový název\nNový obsah";
</script>
```

## Parametry rozhraní

Předejte text Markdown přímo do značky (jako výchozí blok), nebo aktualizujte dynamicky pomocí `value`.

## Popis stylu

Třída stylu `.Md` se automaticky přidá při inicializaci komponenty. Podporuje následující rozložení rozšířeného varovného pole Markdown (spouštěné referenční syntaxí):

- `[!NOTE]`:Okamžité informace
- `[!TIP]`: Návrhy/tipy
- `[!IMPORTANT]`: Důležitá pozornost
- `[!WARNING]`: Varování před rizikem
- `[!CAUTION]`: Upozornění na extrémně vysoké riziko

## bezpečnostní mechanismus

Při vykreslování nejprve zavolejte nativní `setHTML` API, abyste bezpečně vložili HTML. Pokud to prohlížeč nepodporuje, automaticky přejde na nižší verzi na rekurzivní čistič založený na `DOMParser` a implementuje následující bezpečnostní zásady:
- Odstraňte značky, které nejsou bezpečné (např. `script`, `iframe` atd.)
- Vymazat `on*` atributy události
- Chraňte se před útoky XSS filtrováním atributů počínaje nebezpečnými protokoly, jako jsou `javascript:`, `vbscript:` a `data:` (kromě `data:image/`).