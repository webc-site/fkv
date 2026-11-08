# Búðu til og sýndu sprettiglugga með lokahnappi

- Virkt símtal til að búa til og sýna sprettigluggalag
- Innbyggður lokunarhnappur, smelltu til að loka og fjarlægja þáttinn úr DOM
- Ýttu á Esc til að stilla inntaksreitinn úr fókus eða loka sprettigluggalaginu

## Notaðu kynninguna

```html
<button>Smelltu til að poppa upp</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "texti";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Viðmótsbreytur

Sjálfgefin útflutningsaðgerð skilar `HTMLDialogElement` og `Box` og `X` stílflokkunum hefur verið bætt við gluggaeininguna.

## stílaflokkur

### `.Box.X`

Notað á `dialog` þáttinn, tilgreinir gluggastílinn.

### `a.X`

Notað á `a` þáttinn, tilgreinir lokahnappastílinn.