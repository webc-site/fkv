# Creeu i visualitzeu una capa emergent amb un botó de tancament

- Crida funcional per crear i mostrar la capa emergent
- Botó de tancament integrat, feu clic per tancar i eliminar l'element del DOM
- Premeu Esc per desenfocar el quadre d'entrada o tancar la capa emergent

## Utilitzeu la demostració

```html
<button>Feu clic per aparèixer</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "text";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Paràmetres de la interfície

La funció d'exportació predeterminada retorna `HTMLDialogElement`, i les classes d'estil `Box` i `X` s'han afegit a l'element de diàleg.

## classe d'estil

### `.Box.X`

Aplicat a l'element `dialog`, especifica l'estil de diàleg.

### `a.X`

Aplicat a l'element `a`, especifica l'estil del botó de tancament.