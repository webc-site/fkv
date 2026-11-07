# Create and display a popup layer with a close button

- Functional call to create and display popup layer
- Built-in close button, click to close and remove the element from the DOM
- Press Esc to defocus the input box or close the popup layer

## Use the demo

```html
<button>Click to pop up</button>

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


## Interface parameters

The default export function returns `HTMLDialogElement`, and the `Box` and `X` style classes have been added to the dialog element.

## Style Class

### `.Box.X`

Applied to the `dialog` element, specifies the dialog style.

### `a.X`

Applied to the `a` element, specifies the close button style.