# ujuva mulli viip

Kui hiirt hõljutatakse või element on aktiveeritud, kuvage hõljumise kohtspikri teave.

- Põhineb natiivsel veebikomponendil, kerge ja suure jõudlusega
- Toetab adaptiivset ruumilist positsioneerimist neljas suunas (class="top/bottom/left/right")
- See kasutab musta mattklaasi tekstuuri gradiendi tausta ja valgeid märke koos oivaliste füüsiliste servakontuuridega.
- Sisseehitatud kohanduv arvutus, kohandab hüpikakna suunda automaatselt vastavalt vaateaknas allesjäänud ruumile
- Kasutage ujuva viipasisu kuvamiseks `slot="pop"`

## Kasutage demo

```html
<!-- Arvutage suund automaatselt (soovitatav, klassi pole vaja deklareerida) -->
<c-pophover>
  <button>automaatne</button>
  <div slot="pop">See on mulliots</div>
</c-pophover>

<!-- fikseeritud suund -->
<c-pophover class="left">
  <button>Vasakule</button>
  <div slot="pop">See on mulliots</div>
</c-pophover>

<c-pophover class="right">
  <button>õige</button>
  <div slot="pop">See on mulliots</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Alla</button>
  <div slot="pop">See on mulliots</div>
</c-pophover>

<c-pophover class="top">
  <button>ülemus</button>
  <div slot="pop">See on mulliots</div>
</c-pophover>
```

## CSS-i muutujad

Kohandatud stiile toetatakse järgmiste CSS-i muutujate kaudu:

| muutuja nimi | vaikeväärtus | illustreerima |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Mulli tööriistaotsiku ja käivitava elemendi vaheline ruum |
| `--pophover-margin` | `24px` | Vaateava serva ohutusvaru adaptiivse positsioneerimise arvutamisel |
| `--pophover-bg-top` | `#18181ce0` | Mulligradiendi tausta ülemine värv |
| `--pophover-bg-mid` | `#121215e5` | Mulli vasakul ja paremal küljel olevate noolte taustavärv |
| `--pophover-bg-bottom` | `#0c0c0feb` | Mulli gradiendi tausta alumise ning ülemise ja alumise külgnoolte värv |

## Ikooni tööriistavihje stiil (`.Ico`)

Puhast ikoonitüüpi ujuva viipa nuppude jaoks (nt "Muuda", "Kustuta" jne) saate rakendada ühtse ikooni ja hõljutusstiili, lisades välisele konteinerile või komponendile klassi nime `.Ico`.

### Struktuuri näide

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">muuda</div>
  </c-pophover>
</b>
```

Põhipaigutus, suurus (18 x 18 pikslit) ja hõljumise värvimuutused on dokumendis `.Ico > c-pophover > a` ühtlustatud. Arendajad peavad ainult vastava ikoonifaili eraldi määrama, näiteks:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```