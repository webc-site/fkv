# plutajući oblačić prompt

Prikaz informacija o alatnom opisu lebdenja kada lebdite mišem ili je element aktiviran.

- Temeljen na izvornoj web komponenti, lagan i visokih performansi
- Podržava prilagodljivo prostorno pozicioniranje u četiri smjera (class="top/bottom/left/right")
- Koristi gradijentnu pozadinu teksture od crnog mat stakla i bijele znakove, s izvrsnim istaknutim fizičkim rubovima.
- Ugrađeni prilagodljivi izračun položaja, automatski prilagođava smjer skočnog prozora prema preostalom prostoru u prozoru
- Koristite `slot="pop"` za prikaz plutajućeg sadržaja upita

## Koristite demo

```html
<!-- Automatski izračunaj smjer (preporučeno, nema potrebe za deklariranjem klase) -->
<c-pophover>
  <button>automatski</button>
  <div slot="pop">Ovo je vrh s mjehurićima</div>
</c-pophover>

<!-- fiksni smjer -->
<c-pophover class="left">
  <button>Lijevo</button>
  <div slot="pop">Ovo je vrh s mjehurićima</div>
</c-pophover>

<c-pophover class="right">
  <button>pravo</button>
  <div slot="pop">Ovo je vrh s mjehurićima</div>
</c-pophover>

<c-pophover class="bottom">
  <button>dolje</button>
  <div slot="pop">Ovo je vrh s mjehurićima</div>
</c-pophover>

<c-pophover class="top">
  <button>superioran</button>
  <div slot="pop">Ovo je vrh s mjehurićima</div>
</c-pophover>
```

## CSS varijable

Prilagođeni stilovi podržani su putem sljedećih CSS varijabli:

| naziv varijable | zadana vrijednost | ilustrirati |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Prostor između oblačića i elementa za okidanje |
| `--pophover-margin` | `24px` | Sigurnosna margina ruba prozora za prikaz tijekom izračuna prilagodljivog pozicioniranja |
| `--pophover-bg-top` | `#18181ce0` | Gradijent mjehurića u gornjoj boji pozadine |
| `--pophover-bg-mid` | `#121215e5` | Boja pozadine strelica na lijevoj i desnoj strani mjehurića |
| `--pophover-bg-bottom` | `#0c0c0feb` | Boja donje i gornje i donje bočne strelice gradijentne pozadine mjehurića |

## Stil opisa ikone (`.Ico`)

Za plutajuće promptne gumbe čiste ikone (kao što su "Uredi", "Izbriši" itd.), možete primijeniti jedinstvenu ikonu i stil lebdenja dodavanjem naziva klase `.Ico` vanjskom spremniku ili samoj komponenti.

### Primjer strukture

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">urediti</div>
  </c-pophover>
</b>
```

Osnovni izgled, veličina (18x18px) i promjene boje pri lebdenju unificirani su u `.Ico > c-pophover > a`. Programeri samo trebaju posebno navesti odgovarajuću datoteku ikone, kao što je:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```