# naglutaw nga bubble prompt

Ipakita ang impormasyon sa hover tooltip kung ang mouse gipalupad o ang elemento gi-aktibo.

- Base sa lumad nga Web Component, gaan ug taas nga performance
- Nagsuporta sa adaptive spatial positioning sa upat ka direksyon (class="top/bottom/left/right")
- Naggamit kini usa ka itom nga frosted glass texture gradient background ug puti nga mga karakter, nga adunay matahum nga pisikal nga sulud sa sulud sa sulud.
- Ang built-in nga posisyon nga adaptive nga kalkulasyon, awtomatiko nga nag-adjust sa pop-up nga direksyon sumala sa nahabilin nga wanang sa viewport
- Gamita ang `slot="pop"` aron ipakita ang naglutaw nga dali nga sulud

## Gamita ang demo

```html
<!-- Awtomatikong kalkulado ang direksyon (girekomenda, dili kinahanglan nga ipahayag ang klase) -->
<c-pophover>
  <button>awtomatiko</button>
  <div slot="pop">Kini usa ka tip sa bula</div>
</c-pophover>

<!-- pirmi nga direksyon -->
<c-pophover class="left">
  <button>Wala</button>
  <div slot="pop">Kini usa ka tip sa bula</div>
</c-pophover>

<c-pophover class="right">
  <button>husto</button>
  <div slot="pop">Kini usa ka tip sa bula</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Sa ubos</button>
  <div slot="pop">Kini usa ka tip sa bula</div>
</c-pophover>

<c-pophover class="top">
  <button>labaw</button>
  <div slot="pop">Kini usa ka tip sa bula</div>
</c-pophover>
```

## CSS variables

Gisuportahan ang mga custom nga istilo pinaagi sa mosunod nga mga variable sa CSS:

| variable nga ngalan | default nga bili | iilustrar |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Ang luna tali sa bubble tooltip ug sa triggering element |
| `--pophover-margin` | `24px` | Viewport edge safety margin atol sa adaptive positioning kalkulasyon |
| `--pophover-bg-top` | `#18181ce0` | Bubble gradient nga background top color |
| `--pophover-bg-mid` | `#121215e5` | Ang kolor sa background sa mga pana sa wala ug tuo nga kilid sa bula |
| `--pophover-bg-bottom` | `#0c0c0feb` | Ang kolor sa ubos ug taas ug ubos nga mga arrow sa kilid sa background sa bubble gradient |

## Estilo sa tooltip sa icon (`.Ico`)

Para sa puro nga icon type nga floating prompt buttons (sama sa "Edit", "Delete", etc.), mahimo nimong i-apply ang usa ka unified icon ug hover style pinaagi sa pagdugang sa `.Ico` class name ngadto sa gawas nga sudlanan o component mismo.

### Pananglitan sa istruktura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">usba</div>
  </c-pophover>
</b>
```

Ang batakang layout, gidak-on (18x18px), ug mga kausaban sa kolor sa hover nahiusa sa `.Ico > c-pophover > a`. Kinahanglan lang nga ipiho sa mga developer ang katugbang nga file sa icon nga gilain, sama sa:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```