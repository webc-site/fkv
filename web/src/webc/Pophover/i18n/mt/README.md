# bużżieqa f'wiċċ l-ilma fil-pront

Uri l-informazzjoni dwar l-għodda tal-hover meta l-maws jitmexxa 'l fuq jew l-element jiġi attivat.

- Ibbażat fuq Komponent tal-Web nattiv, ħfief u ta 'prestazzjoni għolja
- Jappoġġja pożizzjonament spazjali adattiv f'erba 'direzzjonijiet (klassi = "fuq / isfel / xellug / lemin")
- Juża sfond tal-gradjent tan-nisġa tal-ħġieġ matt iswed u karattri bojod, b'punti ewlenin tal-kontorn tat-tarf fiżiku exquisite.
- Kalkolu adattiv tal-pożizzjoni inkorporat, jaġġusta awtomatikament id-direzzjoni tal-pop-up skont l-ispazju li jifdal fil-viewport
- Uża `slot="pop"` biex turi kontenut f'wiċċ l-ilma fil-pront

## Uża d-demo

```html
<!-- Ikkalkula awtomatikament id-direzzjoni (rakkomandat, m'hemmx għalfejn tiddikjara l-klassi) -->
<c-pophover>
  <button>awtomatiku</button>
  <div slot="pop">Din hija ponta tal-bużżieqa</div>
</c-pophover>

<!-- direzzjoni fissa -->
<c-pophover class="left">
  <button>Xellug</button>
  <div slot="pop">Din hija ponta tal-bużżieqa</div>
</c-pophover>

<c-pophover class="right">
  <button>dritt</button>
  <div slot="pop">Din hija ponta tal-bużżieqa</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Down</button>
  <div slot="pop">Din hija ponta tal-bużżieqa</div>
</c-pophover>

<c-pophover class="top">
  <button>superjuri</button>
  <div slot="pop">Din hija ponta tal-bużżieqa</div>
</c-pophover>
```

## Varjabbli CSS

L-istili personalizzati huma appoġġjati permezz tal-varjabbli CSS li ġejjin:

| isem varjabbli | valur default | juru |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | L-ispazju bejn it-tooltip tal-bużżieqa u l-element triggering |
| `--pophover-margin` | `24px` | Marġini tas-sigurtà tat-tarf tal-viewport waqt il-kalkolu tal-pożizzjonament adattiv |
| `--pophover-bg-top` | `#18181ce0` | Kulur ta 'fuq fl-isfond gradjent tal-bużżieqa |
| `--pophover-bg-mid` | `#121215e5` | Il-kulur fl-isfond tal-vleġeġ fuq in-naħat tax-xellug u tal-lemin tal-bużżieqa |
| `--pophover-bg-bottom` | `#0c0c0feb` | Il-kulur tal-vleġeġ tal-qiegħ u tan-naħa ta 'fuq u t'isfel tal-isfond tal-gradjent tal-bużżieqa |

## Stil tooltip tal-ikona (`.Ico`)

Għal buttuni fil-pront li jżommu f'wiċċ l-ilma tat-tip ta' ikona pura (bħal "Editja", "Ħassar", eċċ.), tista' tapplika ikona unifikata u stil ta 'hover billi żżid l-isem tal-klassi `.Ico` mal-kontenitur ta' barra jew komponent innifsu.

### Eżempju ta' struttura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">editja</div>
  </c-pophover>
</b>
```

It-tqassim bażiku, id-daqs (18x18px), u l-bidliet fil-kulur tal-hover ġew unifikati f'`.Ico > c-pophover > a`. L-iżviluppaturi jeħtieġ biss li jispeċifikaw il-fajl tal-ikona korrispondenti separatament, bħal:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```