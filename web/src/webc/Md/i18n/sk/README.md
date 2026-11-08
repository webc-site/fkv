# Bezpečne analyzujte a vykresľte text Markdown do HTML.

- **Bezpečnostná dezinfekcia**: Chráňte sa pred XSS útokmi pomocou natívnych `setHTML` API alebo `DOMParser` čističov
- ** Extrakcia obsahu**: Automaticky prečítať textový obsah predvoleného slotu počas inicializácie a vykresliť ho
- **Vstavaná sadzba**: štýly prednastaveného zoznamu, tabuľky, bloku kódu, ponuky a varovného poľa

## Použite demo

### statické vykresľovanie

```html
<c-md>
# titul
Toto je časť obsahu Markdown
</c-md>
```

### dynamické priradenie

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nový názov\nNový obsah";
</script>
```

## Parametre rozhrania

Odovzdajte text Markdown priamo v rámci značky (ako predvolený priestor) alebo aktualizujte dynamicky prostredníctvom `value`.

## Popis štýlu

Trieda štýlu `.Md` sa automaticky pridá pri inicializácii komponentu. Podporuje nasledujúce rozloženie rozšíreného varovného poľa Markdown (spúšťané referenčnou syntaxou):

- `[!NOTE]`:Promptné informácie
- `[!TIP]`: Návrhy/tipy
- `[!IMPORTANT]`: Dôležitá pozornosť
- `[!WARNING]`: Varovanie pred rizikom
- `[!CAUTION]`: Upozornenie na extrémne vysoké riziko

## bezpečnostný mechanizmus

Pri vykresľovaní najprv zavolajte natívne `setHTML` API, aby ste bezpečne vložili HTML. Ak to prehliadač nepodporuje, automaticky prejde na nižší rekurzívny čistič založený na `DOMParser` a implementuje nasledujúcu bezpečnostnú politiku:
- Odstráňte značky, ktoré nie sú bezpečné (napr. `script`, `iframe` atď.)
- Vymazať `on*` atribúty udalosti
- Chráňte sa pred XSS útokmi filtrovaním atribútov počnúc nebezpečnými protokolmi, ako sú `javascript:`, `vbscript:` a `data:` (okrem `data:image/`).