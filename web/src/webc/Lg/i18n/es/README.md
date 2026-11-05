# Fondo de vidrio esmerilado y efecto de resaltado de borde

El borde semitransparente y la sombra interior crean reflejos en los bordes en microrrelieve.
Como dependencia de estilo básica de los componentes de botón y cuadro de entrada.

## Usa la demostración

```html
<main class="Lg">
  <article>
    <h3>título</h3>
    <pre>Texto de contenido</pre>
  </article>
</main>
```

## clase de estilo

### `.Lg`
Estilo básico de tarjeta. Aplica `backdrop-filter` desenfoque, `border` borde con microrrelieve y `box-shadow` sombra interior. Aplique el filtro `svg/glass.svg#h` al pasar el mouse.

### `main.Lg`
El contenedor de bloques principal de la página. Esquinas redondeadas `24px` con relleno sensible (`16px` si el ancho es menor que `800px`, `24px` en caso contrario).

### `.Btn`, `.BtnC`, `.Input`
Nombre de clase de componente básico. Después de introducir este estilo, obtendrá automáticamente un diseño flexible, relaciones jerárquicas y propiedades básicas del vidrio esmerilado.