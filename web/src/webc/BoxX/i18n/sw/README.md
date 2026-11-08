# Unda na uonyeshe safu ibukizi na kitufe cha kufunga

- Simu inayofanya kazi ili kuunda na kuonyesha safu ibukizi
- Kitufe cha kufunga kilichojengwa ndani, bofya ili kufunga na kuondoa kipengele kutoka kwa DOM
- Bonyeza Esc ili kupunguza umakini kwenye kisanduku cha ingizo au funga safu ibukizi

## Tumia onyesho

```html
<button>Bofya ili pop up</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "maandishi";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Vigezo vya interface

Chaguo chaguo-msingi za kukokotoa hurejesha `HTMLDialogElement`, na `Box` na `X` madarasa ya mtindo yameongezwa kwenye kipengele cha mazungumzo.

## darasa la mtindo

### `.Box.X`

Imetumika kwa kipengele cha `dialog`, inabainisha mtindo wa kidadisi.

### `a.X`

Imetumika kwa kipengele cha `a`, inabainisha mtindo wa kitufe cha kufunga.