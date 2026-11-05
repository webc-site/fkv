# Fondo de vidro esmerilado e efecto de resaltado de bordo

O bordo semitransparente e a sombra interior crean puntos destacados de microrrelevo.
Como a dependencia de estilo básico dos compoñentes do botón e da caixa de entrada.

## Use a demostración

```html
<main class="Lg">
  <article>
    <h3>título</h3>
    <pre>Texto de contido</pre>
  </article>
</main>
```

## clase de estilo

### `.Lg`
Estilo básico de tarxeta. Aplica `backdrop-filter` desenfoque, `border` bordo micro-relevado e `box-shadow` sombra interior. Aplicar o filtro `svg/glass.svg#h` ao pasar o cursor.

### `main.Lg`
Contedor de bloques principal da páxina. Esquinas redondeadas `24px` con recheo sensible (`16px` se o ancho é inferior a `800px`, `24px` en caso contrario).

### `.Btn`, `.BtnC`, `.Input`
Nome da clase de compoñente básico. Despois de introducir este estilo, obterá automaticamente un deseño flexible, relacións xerárquicas e propiedades básicas do vidro esmerilado.