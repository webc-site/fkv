# prompt cu bule plutitoare

Afișează informațiile de tip tooltip pentru trecerea cursorului când mouse-ul este plasat sau elementul este activat.

- Bazat pe Componenta Web nativă, ușoară și de înaltă performanță
- Sprijină poziționarea spațială adaptivă în patru direcții (clas="sus/jos/stânga/dreapta")
- Folosește un fundal degradat de textură de sticlă mată neagră și caractere albe, cu evidențieri fizice rafinate de contur.
- Calcul adaptiv al poziției încorporat, ajustează automat direcția pop-up în funcție de spațiul rămas în fereastra de vizualizare
- Utilizați `slot="pop"` pentru a afișa conținutul prompt flotant

## Utilizați demonstrația

```html
<!-- Calculați automat direcția (recomandat, nu este nevoie să declarați clasa) -->
<c-pophover>
  <button>automat</button>
  <div slot="pop">Acesta este un vârf de bule</div>
</c-pophover>

<!-- direcție fixă -->
<c-pophover class="left">
  <button>Stânga</button>
  <div slot="pop">Acesta este un vârf de bule</div>
</c-pophover>

<c-pophover class="right">
  <button>corect</button>
  <div slot="pop">Acesta este un vârf de bule</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Jos</button>
  <div slot="pop">Acesta este un vârf de bule</div>
</c-pophover>

<c-pophover class="top">
  <button>superior</button>
  <div slot="pop">Acesta este un vârf de bule</div>
</c-pophover>
```

## variabile CSS

Stilurile personalizate sunt acceptate prin următoarele variabile CSS:

| numele variabilei | valoare implicită | ilustra |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Spațiul dintre balonul indicativ și elementul de declanșare |
| `--pophover-margin` | `24px` | Marja de siguranță la marginea ferestrei în timpul calculului de poziționare adaptivă |
| `--pophover-bg-top` | `#18181ce0` | Culoare de fundal cu gradient de bule |
| `--pophover-bg-mid` | `#121215e5` | Culoarea de fundal a săgeților din partea stângă și dreaptă a bulei |
| `--pophover-bg-bottom` | `#0c0c0feb` | Culoarea săgeților laterale de jos și de sus și de jos ale fundalului cu gradient de bule |

## Stil indicativ pentru pictograme (`.Ico`)

Pentru butoanele de prompt plutitoare de tip pictogramă pură (cum ar fi „Editare”, „Ștergere”, etc.), puteți aplica o pictogramă unificată și un stil de trecere cu mouse-ul adăugând numele clasei `.Ico` la containerul exterior sau componenta în sine.

### Exemplu de structură

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">edita</div>
  </c-pophover>
</b>
```

Aspectul de bază, dimensiunea (18x18px) și modificările de culoare pentru trecerea cursorului au fost unificate în `.Ico > c-pophover > a`. Dezvoltatorii trebuie doar să specifice separat fișierul pictogramă corespunzător, cum ar fi:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```