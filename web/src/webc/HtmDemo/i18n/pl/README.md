# Debugowanie kodu HTML i JS online

- Wyrenderuj pasek debugowania pod kodem i udostępnij portale debugowania CodePen i JSFiddle
- Automatycznie zapamiętuj wybraną platformę debugowania
- Obsługuje wstrzykiwanie wywołań zwrotnych poprzez atrybut `.gen` w celu dynamicznego generowania kodu HTML, CSS i JS
- Automatycznie identyfikuj elementy podrzędne z nazwą klasy `language-js` i uruchamiaj w trybie JS

## Skorzystaj z wersji demonstracyjnej

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demonstracja</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Powrót [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametry interfejsu

### Właściwości i metody

- `.gen`: Wstrzyknięta funkcja wywołania zwrotnego. ** To wywołanie zwrotne musi zostać wstrzyknięte, aby pasek debugowania został wyświetlony. **
  - parametr:
    - `type`: typ wstrzykniętej treści (`HTML = 1`, `JS = 2`).
    - `text`: Tekst kodu wyodrębniony z elementu podrzędnego.
  - Wartość zwracana: `[htm, css, js]`