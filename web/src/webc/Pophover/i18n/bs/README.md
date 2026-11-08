# plutajući balon prompt

Prikažite informacije o oblačiću kada se miš pređe mišem ili kada je element aktiviran.

- Zasnovan na izvornoj web komponenti, lagan i visokih performansi
- Podržava prilagodljivo prostorno pozicioniranje u četiri smjera (class="top/bottom/left/right")
- Koristi pozadinu s gradijentom teksture crnog matiranog stakla i bijele znakove, sa izuzetnim fizičkim naglašavanjima kontura ivica.
- Ugrađeni proračun za prilagođavanje položaja, automatski prilagođava smjer iskačućih prozora prema preostalom prostoru u prozoru za prikaz
- Koristite `slot="pop"` za prikaz plutajućeg sadržaja upita

## Koristite demo

```html
<!-- Automatski izračunaj smjer (preporučeno, nije potrebno deklarirati klasu) -->
<c-pophover>
  <button>automatski</button>
  <div slot="pop">Ovo je mjehurić vrh</div>
</c-pophover>

<!-- fiksni pravac -->
<c-pophover class="left">
  <button>lijevo</button>
  <div slot="pop">Ovo je mjehurić vrh</div>
</c-pophover>

<c-pophover class="right">
  <button>u pravu</button>
  <div slot="pop">Ovo je mjehurić vrh</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Dole</button>
  <div slot="pop">Ovo je mjehurić vrh</div>
</c-pophover>

<c-pophover class="top">
  <button>superioran</button>
  <div slot="pop">Ovo je mjehurić vrh</div>
</c-pophover>
```

## CSS varijable

Prilagođeni stilovi su podržani putem sljedećih CSS varijabli:

| ime varijable | zadana vrijednost | ilustrirati |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Prostor između oblačića i elementa za okidanje |
| `--pophover-margin` | `24px` | Sigurnosna margina ruba prikaza tokom proračuna prilagodljivog pozicioniranja |
| `--pophover-bg-top` | `#18181ce0` | Boja gornje pozadine gradijenta mjehurića |
| `--pophover-bg-mid` | `#121215e5` | Boja pozadine strelica na lijevoj i desnoj strani oblačića |
| `--pophover-bg-bottom` | `#0c0c0feb` | Boja donje i gornje i donje bočne strelice pozadine gradijenta mjehurića |

## Stil opisa ikone (`.Ico`)

Za plutajuću brzu dugmad čistog tipa ikone (kao što je "Uredi", "Izbriši" itd.), možete primijeniti jedinstvenu ikonu i lebdeći stil dodavanjem naziva klase `.Ico` vanjskom spremniku ili samoj komponenti.

### Primjer strukture

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">edit</div>
  </c-pophover>
</b>
```

Osnovni izgled, veličina (18x18px) i promjene boje pri lebdenju su objedinjene u `.Ico > c-pophover > a`. Programeri samo trebaju zasebno specificirati odgovarajuću datoteku ikona, kao što su:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```