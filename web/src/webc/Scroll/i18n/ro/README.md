# bara de defilare virtuală

- **Defilare în două direcții**: oferă defilare verticală (`c-vs`) și orizontală (`c-hs`).
- **Răspuns la dimensiune**: monitorizează modificările de conținut și dimensiunea zonei, actualizează automat dimensiunea și poziția barei de defilare și ascunde automat atunci când nu există depășire.
- **Suport interactiv**: acceptă derularea roții, glisarea glisorului și poziționarea clicului pe pistă.
- **Ascundeți starea**: afișați și lărgiți glisorul atunci când treceți cu mouse-ul sau derulați și ascundeți-l automat când stați.

## defilare verticală

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## defilare orizontală

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## piese de stil

Personalizați stilul elementelor interne Shadow DOM prin `::part()`:

- `::part(scroll)`: derulați zona de conținut.
- `::part(bar)`: piesă din bara de defilare.
- `::part(si)`: glisor pentru bara de defilare.
- `::part(i)`: strat de ambalare intern.
- `::part(drag)`: pista barei de defilare în starea de glisare.

## clasa de stil

- `.drag`: aplicat elementului `body`, indicând faptul că acesta se află într-o stare de derulare prin glisare.

## Proprietăți personalizate

- `--si-bg`: culoarea de fundal a cursorului.
- `--si-anim`: animație cu glisor.
- `--cursorScrollhSvg`: cursorul mouse-ului când bara de defilare orizontală este plasată.
- `--cursorScrollvSvg`: cursorul mouse-ului când bara de defilare verticală este plasată.