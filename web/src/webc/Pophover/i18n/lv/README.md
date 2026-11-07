# peldoša burbuļa uzvedne

Rādīt kursora padoma informāciju, kad pele tiek virzīta uz peles kursoru vai elements ir aktivizēts.

- Pamatojoties uz vietējo Web komponentu, viegls un augstas veiktspējas
- Atbalsta adaptīvo telpisko pozicionēšanu četros virzienos (class="top/bottom/left/right")
- Tajā ir izmantots melna matēta stikla tekstūras gradienta fons un baltas rakstzīmes ar izsmalcinātu fizisko malu kontūru.
- Iebūvēts pozīcijas adaptīvais aprēķins, automātiski pielāgo uznirstošo logu virzienu atbilstoši atlikušajai vietai skata logā
- Izmantojiet `slot="pop"`, lai parādītu peldošu uzvednes saturu

## Izmantojiet demonstrāciju

```html
<!-- Automātiski aprēķināt virzienu (ieteicams, nav nepieciešams deklarēt klasi) -->
<c-pophover>
  <button>automātiski</button>
  <div slot="pop">Šis ir burbuļa gals</div>
</c-pophover>

<!-- fiksēts virziens -->
<c-pophover class="left">
  <button>Pa kreisi</button>
  <div slot="pop">Šis ir burbuļa gals</div>
</c-pophover>

<c-pophover class="right">
  <button>pareizi</button>
  <div slot="pop">Šis ir burbuļa gals</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Uz leju</button>
  <div slot="pop">Šis ir burbuļa gals</div>
</c-pophover>

<c-pophover class="top">
  <button>pārāks</button>
  <div slot="pop">Šis ir burbuļa gals</div>
</c-pophover>
```

## CSS mainīgie

Pielāgotie stili tiek atbalstīti, izmantojot šādus CSS mainīgos:

| mainīgais nosaukums | noklusējuma vērtība | ilustrēt |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Atstarpe starp burbuļa rīka padomu un palaišanas elementu |
| `--pophover-margin` | `24px` | Skatloga malas drošības rezerve adaptīvās pozicionēšanas aprēķina laikā |
| `--pophover-bg-top` | `#18181ce0` | Burbuļa gradienta fona augšējā krāsa |
| `--pophover-bg-mid` | `#121215e5` | Burbuļa kreisajā un labajā pusē esošo bultu fona krāsa |
| `--pophover-bg-bottom` | `#0c0c0feb` | Burbuļa gradienta fona apakšējās un augšējās un apakšējās sānu bultiņas krāsa |

## Ikonas rīka padoma stils (`.Ico`)

Tīra ikonu tipa peldošām uzvednes pogām (piemēram, "Rediģēt", "Dzēst" utt.) varat lietot vienotu ikonu un virzīšanas stilu, pievienojot `.Ico` klases nosaukumu pašam ārējam konteineram vai komponentam.

### Struktūras piemērs

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">rediģēt</div>
  </c-pophover>
</b>
```

Pamata izkārtojums, izmērs (18 x 18 pikseļi) un kursora virzīšanas krāsas izmaiņas ir apvienotas sadaļā `.Ico > c-pophover > a`. Izstrādātājiem tikai atsevišķi jānorāda atbilstošais ikonas fails, piemēram:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```