# Erstellt a weist eng Popup-Schicht mat engem Zoumaache Knäppchen

- Funktionell Uruff fir Popupschicht ze kreéieren an ze weisen
- Built-in Zoumaache Knäppchen, klickt fir zouzemaachen an d'Element aus der DOM ze läschen
- Dréckt Esc fir d'Inputbox ze defokuséieren oder d'Popupschicht zoumaachen

## Benotzt d'Demo

```html
<button>Klickt fir opzemaachen</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "Text";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interface Parameteren

D'Standardexportfunktioun gëtt `HTMLDialogElement` zréck, an d'`Box` an `X` Stilklassen sinn an den Dialogelement bäigefüügt.

## Stil Klass

### `.Box.X`

Op den `dialog` Element ugewannt, spezifizéiert den Dialogstil.

### `a.X`

Op den `a` Element ugewannt, spezifizéiert den Zoumaache Knäppchenstil.