# Depuración de código HTML e JS en liña

- Mostra unha barra de depuración debaixo do código e proporciona portais de depuración CodePen e JSFiddle
- Lembra automaticamente a plataforma de depuración seleccionada
- Admite a inxección de devolucións de chamada a través do atributo `.gen` para xerar de forma dinámica código HTML, CSS e JS
- Identifica automaticamente os elementos fillos co nome de clase `language-js` e executa en modo JS

## Use a demostración

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demostración</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Retorno [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parámetros da interface

### Propiedades e métodos

- `.gen`: función de devolución de chamada inxectada. **Esta devolución de chamada debe ser inxectada para que se mostre a barra de depuración. **
  - parámetro:
    - `type`: tipo de contido inxectado (`HTML = 1`, `JS = 2`).
    - `text`: o texto do código extraído do elemento fillo.
  - Valor de retorno: `[htm, css, js]`