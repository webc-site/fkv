# Sortu eta bistaratu laster-geruza

- Popup geruza sortzeko eta bistaratzeko dei funtzionala
- Kendu automatikoki DOMetik elementuak itxita dagoenean
- Saihestu Esc tekla bertan behera uzteko portaera

## Erabili demoa

```html
<button>Egin klik agertzeko</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Izenburua";
  description.textContent = "Testua";
  button.className = "Btn Main";
  button.textContent = "Ados";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interfazearen parametroak

### Esportatutako funtzio lehenetsiak

`Box()`

- **Itzuli balioa**: `HTMLDialogElement`, `Box` estilo klaseko elkarrizketa-elementua gehitu da.

## estilo klasea

### `.Box`

`dialog` elementuari aplikatzen zaio kokapena, zentrazioa eta atzeko planoko maskara lausotzea ezartzeko.

### `.Lg`

Barneko edukiari aplikatutako estilo-klase bat, atzeko planoaren kolorea, ertz biribilduak, itzala eta diseinu bertikala ezarriz eta barneko `h3` eta `p` diseinua definituz.