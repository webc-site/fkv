# Luo ja näytä ponnahdusikkuna taso sulkemispainikkeella

- Toiminnallinen kutsu ponnahdusikkunakerroksen luomiseen ja näyttämiseen
- Sisäänrakennettu sulkemispainike, napsauta sulkeaksesi ja poistaaksesi elementin DOM:sta
- Paina Esc muuttaaksesi syöttöruutua tai sulkeaksesi ponnahdusikkunan

## Käytä demoa

```html
<button>Napsauta avataksesi ponnahdusikkunan</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "teksti";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Käyttöliittymän parametrit

Oletusvientifunktio palauttaa `HTMLDialogElement`, ja tyyliluokat `Box` ja `X` on lisätty valintaikkunaelementtiin.

## tyyliluokka

### `.Box.X`

Sovelletaan `dialog` -elementtiin, määrittää valintaikkunan tyylin.

### `a.X`

Sovelletaan `a` -elementtiin, määrittää sulkemispainikkeen tyylin.