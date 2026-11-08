# barra de desplazamiento virtual

- **Desplazamiento bidireccional**: proporciona desplazamiento vertical (`c-vs`) y horizontal (`c-hs`).
- **Respuesta de tamaño**: supervise los cambios de contenido y tamaño del área, actualice automáticamente el tamaño y la posición de la barra de desplazamiento y oculte automáticamente cuando no haya desbordamiento.
- **Soporte interactivo**: Admite desplazamiento con rueda, arrastre de control deslizante y posicionamiento de clic en pista.
- **Ocultar estado**: muestra y amplía el control deslizante cuando pasas el cursor o te desplazas, y lo oculta automáticamente cuando estás parado.

## desplazamiento vertical

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

## desplazamiento horizontal

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

## piezas de estilo

Personaliza el estilo de los elementos internos de Shadow DOM mediante `::part()`:

- `::part(scroll)`: desplazarse por el área de contenido.
- `::part(bar)`: Pista de la barra de desplazamiento.
- `::part(si)`: control deslizante de la barra de desplazamiento.
- `::part(i)`: Capa envolvente interna.
- `::part(drag)`: La pista de la barra de desplazamiento en el estado de arrastre.

## clase de estilo

- `.drag`: se aplica al elemento `body`, lo que indica que está en un estado de arrastre y desplazamiento.

## Propiedades personalizadas

- `--si-bg`: Color de fondo del control deslizante.
- `--si-anim`: Animación deslizante.
- `--cursorScrollhSvg`: El cursor del mouse cuando se coloca el cursor sobre la barra de desplazamiento horizontal.
- `--cursorScrollvSvg`: El cursor del mouse cuando se coloca el cursor sobre la barra de desplazamiento vertical.