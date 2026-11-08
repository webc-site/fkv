# Depuració de codi HTML i JS en línia

- Representeu una barra de depuració a sota del codi i proporcioneu els portals de depuració CodePen i JSFiddle
- Recordeu automàticament la plataforma de depuració seleccionada
- Admet la injecció de devolucions de trucada mitjançant l'atribut `.gen` per generar dinàmicament codi HTML, CSS i JS
- Identifiqueu automàticament els elements secundaris amb `language-js` nom de classe i executeu-los en mode JS

## Utilitzeu la demostració

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demostració</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Torna [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Paràmetres de la interfície

### Propietats i mètodes

- `.gen`: funció de devolució de trucada injectada. **Aquesta devolució de trucada s'ha d'injectar perquè es mostri la barra de depuració. **
  - paràmetre:
    - `type`: tipus de contingut injectat (`HTML = 1`, `JS = 2`).
    - `text`: el text del codi extret de l'element secundari.
  - Valor de retorn: `[htm, css, js]`