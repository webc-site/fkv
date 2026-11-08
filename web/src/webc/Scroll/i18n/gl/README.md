# barra de desprazamento virtual

- **Desprazamento bidireccional**: ofrece desprazamento vertical (`c-vs`) e horizontal (`c-hs`).
- **Resposta ao tamaño**: supervisa o contido e os cambios de tamaño da área, actualiza automaticamente o tamaño e a posición da barra de desprazamento e oculta automaticamente cando non hai desbordamento.
- **Soporte interactivo**: admite o desprazamento das rodas, o arrastre do control deslizante e o posicionamento do clic en pista.
- **Ocultar estado**: amosa e amplía o control deslizante ao pasar o cursor ou desprazarse, e ocultalo automaticamente cando está parado.

## desprazamento vertical

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

## rolo horizontal

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

## pezas de estilo

Personaliza o estilo dos elementos internos de Shadow DOM mediante `::part()`:

- `::part(scroll)`: despraza a área de contido.
- `::part(bar)`: pista da barra de desprazamento.
- `::part(si)`: barra de desprazamento.
- `::part(i)`: capa de envoltura interna.
- `::part(drag)`: a pista da barra de desprazamento no estado de arrastramento.

## clase de estilo

- `.drag`: aplicado ao elemento `body`, indicando que se atopa nun estado de desprazamento arrastrado.

## Propiedades personalizadas

- `--si-bg`: cor de fondo do control deslizante.
- `--si-anim`: animación do control deslizante.
- `--cursorScrollhSvg`: o cursor do rato cando se coloca a barra de desprazamento horizontal.
- `--cursorScrollvSvg`: o cursor do rato cando se coloca a barra de desprazamento vertical.