# swigen fel y bo'r angen yn brydlon

Arddangos gwybodaeth cyngor teclyn hofran pan fydd y llygoden yn hofran neu pan fydd yr elfen yn cael ei hactifadu.

- Yn seiliedig ar Gydran Gwe brodorol, ysgafn a pherfformiad uchel
- Yn cefnogi lleoliad gofodol addasol i bedwar cyfeiriad (dosbarth = "top / gwaelod / chwith / dde")
- Mae'n defnyddio cefndir graddiant gwead gwydr barugog du a chymeriadau gwyn, gydag uchafbwyntiau cyfuchlin ymyl corfforol coeth.
- Cyfrifiad addasol safle adeiledig, yn addasu'r cyfeiriad pop-up yn awtomatig yn ôl y gofod sy'n weddill yn y golygfan
- Defnyddiwch `slot="pop"` i ddangos cynnwys anogwr symudol

## Defnyddiwch y demo

```html
<!-- Cyfrifo cyfeiriad yn awtomatig (argymhellir, nid oes angen datgan dosbarth) -->
<c-pophover>
  <button>awtomatig</button>
  <div slot="pop">Mae hwn yn awgrym swigen</div>
</c-pophover>

<!-- cyfeiriad sefydlog -->
<c-pophover class="left">
  <button>Chwith</button>
  <div slot="pop">Mae hwn yn awgrym swigen</div>
</c-pophover>

<c-pophover class="right">
  <button>iawn</button>
  <div slot="pop">Mae hwn yn awgrym swigen</div>
</c-pophover>

<c-pophover class="bottom">
  <button>I lawr</button>
  <div slot="pop">Mae hwn yn awgrym swigen</div>
</c-pophover>

<c-pophover class="top">
  <button>rhagorach</button>
  <div slot="pop">Mae hwn yn awgrym swigen</div>
</c-pophover>
```

## Newidynnau CSS

Cefnogir arddulliau personol trwy'r newidynnau CSS canlynol:

| enw newidiol | gwerth rhagosodedig | darlunio |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Y gofod rhwng y cyngor swigen a'r elfen sbarduno |
| `--pophover-margin` | `24px` | Ymyl diogelwch ymyl Viewport yn ystod cyfrifiad lleoli addasol |
| `--pophover-bg-top` | `#18181ce0` | Lliw top cefndir graddiant swigen |
| `--pophover-bg-mid` | `#121215e5` | Lliw cefndir y saethau ar ochr chwith ac ochr dde'r swigen |
| `--pophover-bg-bottom` | `#0c0c0feb` | Lliw saethau ochr isaf ac uchaf ac isaf y cefndir graddiant swigen |

## Arddull awgrym offer eicon (`.Ico`)

Ar gyfer botymau prydlon fel y bo'r angen o fath eicon pur (fel "Golygu", "Dileu", ac ati), gallwch gymhwyso eicon unedig a steil hofran trwy ychwanegu'r `.Ico` enw dosbarth i'r cynhwysydd allanol neu'r gydran ei hun.

### Enghraifft o strwythur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">golygu</div>
  </c-pophover>
</b>
```

Mae'r cynllun sylfaenol, maint (18x18px), a newidiadau lliw hofran wedi'u huno yn `.Ico > c-pophover > a`. Dim ond y ffeil eicon cyfatebol y mae angen i ddatblygwyr ei nodi ar wahân, megis:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```