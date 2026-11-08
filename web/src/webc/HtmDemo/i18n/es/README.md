# Depuración de código HTML y JS en línea

- Representa una barra de depuración debajo del código y proporciona portales de depuración CodePen y JSFiddle
- Recuerde automáticamente la plataforma de depuración seleccionada
- Admite la inyección de devoluciones de llamada a través del atributo `.gen` para generar dinámicamente código HTML, CSS y JS.
- Identifique automáticamente elementos secundarios con el nombre de clase `language-js` y ejecútelos en modo JS

## Usa la demostración

```html
<c-htm-demo><pre><code class="language-html">
<h1>Manifestación</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Devuelve [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parámetros de interfaz

### Propiedades y métodos

- `.gen`: Función de devolución de llamada inyectada. **Esta devolución de llamada debe inyectarse para que se muestre la barra de depuración. **
  - parámetro:
    - `type`: tipo de contenido inyectado (`HTML = 1`, `JS = 2`).
    - `text`: El texto del código extraído del elemento secundario.
  - Valor de retorno: `[htm, css, js]`