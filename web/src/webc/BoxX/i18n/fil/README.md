# Lumikha at magpakita ng popup layer na may close button

- Functional na tawag para gumawa at magpakita ng popup layer
- Built-in na close button, i-click upang isara at alisin ang elemento mula sa DOM
- Pindutin ang Esc upang i-defocus ang input box o isara ang popup layer

## Gamitin ang demo

```html
<button>I-click para mag-pop up</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "teksto";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Mga parameter ng interface

Ang default na export function ay nagbabalik ng `HTMLDialogElement`, at ang `Box` at `X` style classes ay naidagdag sa dialog element.

## klase ng istilo

### `.Box.X`

Inilapat sa elementong `dialog`, tinutukoy ang istilo ng dialog.

### `a.X`

Inilapat sa elementong `a`, tinutukoy ang istilo ng close button.