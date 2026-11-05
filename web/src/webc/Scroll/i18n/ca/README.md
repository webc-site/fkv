# barra de desplaçament virtual

- **Desplaçament bidireccional**: proporciona un desplaçament vertical (`c-vs`) i horitzontal (`c-hs`).
- **Resposta a la mida**: supervisa els canvis de mida del contingut i de l'àrea, actualitza automàticament la mida i la posició de la barra de desplaçament i s'amaga automàticament quan no hi hagi desbordament.
- **Suport interactiu**: admet el desplaçament de les rodes, l'arrossegament del control lliscant i el posicionament del clic de la pista.
- **Oculta l'estat**: mostra i amplia el control lliscant quan passa el cursor o desplaça't, i amaga-lo automàticament quan estàs parat.

## desplaçament vertical

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

## desplaçament horitzontal

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

## parts d'estil

Personalitzeu l'estil dels elements interns de Shadow DOM mitjançant `::part()`:

- `::part(scroll)`: desplaceu-vos per l'àrea de contingut.
- `::part(bar)`: pista de la barra de desplaçament.
- `::part(si)`: control lliscant de la barra de desplaçament.
- `::part(i)`: capa d'embolcall interna.
- `::part(drag)`: la pista de la barra de desplaçament en estat d'arrossegament.

## classe d'estil

- `.drag`: s'aplica a l'element `body`, que indica que es troba en estat de desplaçament arrossegant.

## Propietats personalitzades

- `--si-bg`: color de fons del control lliscant.
- `--si-anim`: animació del control lliscant.
- `--cursorScrollhSvg`: el cursor del ratolí quan es mou la barra de desplaçament horitzontal.
- `--cursorScrollvSvg`: el cursor del ratolí quan es mou la barra de desplaçament vertical.