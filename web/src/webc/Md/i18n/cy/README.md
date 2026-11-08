# Dosrannu a rendr testun Markdown yn ddiogel i HTML.

- **Glanweithdra Diogelwch**: Amddiffyn rhag ymosodiadau XSS gan ddefnyddio `setHTML` API neu `DOMParser` glanhawyr brodorol
- **Echdynnu Cynnwys**: Darllenwch gynnwys testun y slot rhagosodedig yn awtomatig wrth gychwyn a'i rendrad
- ** Cysodi adeiledig**: rhestr ragosodedig, tabl, bloc cod, arddull dyfynbris a blychau rhybuddio

## Defnyddiwch y demo

### rendro statig

```html
<c-md>
# teitl
Dyma ddarn o gynnwys Markdown
</c-md>
```

### aseiniad deinamig

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Teitl newydd\nCynnwys newydd";
</script>
```

## Paramedrau rhyngwyneb

Pasiwch destun Markdown yn uniongyrchol o fewn y tag (fel y slot rhagosodedig), neu diweddarwch yn ddeinamig trwy `value`.

## Disgrifiad arddull

Mae'r dosbarth arddull `.Md` yn cael ei ychwanegu'n awtomatig pan ddechreuir y gydran. Yn cefnogi'r cynllun blwch rhybuddio estynedig Markdown canlynol (wedi'i ysgogi gan gystrawen gyfeirio):

- `[!NOTE]`: Gwybodaeth brydlon
- `[!TIP]`: Awgrymiadau/Awgrymiadau
- `[!IMPORTANT]`: Sylw pwysig
- `[!WARNING]`: Rhybudd risg
- `[!CAUTION]`: Rhybudd risg hynod o uchel

## mecanwaith diogelwch

Wrth rendro, ffoniwch yr API brodorol `setHTML` yn gyntaf i chwistrellu HTML yn ddiogel. Os nad yw'r porwr yn ei gefnogi, bydd yn israddio'n awtomatig i lanhawr ailadroddus yn seiliedig ar `DOMParser` ac yn gweithredu'r polisi diogelwch canlynol:
- Tynnwch dagiau nad ydynt yn ddiogel (e.e. `script`, `iframe`, ac ati)
- Clirio `on*` priodoleddau digwyddiad
- Amddiffyn rhag ymosodiadau XSS trwy hidlo priodoleddau gan ddechrau gyda phrotocolau peryglus megis `javascript:`, `vbscript:`, a `data:` (ac eithrio `data:image/`).