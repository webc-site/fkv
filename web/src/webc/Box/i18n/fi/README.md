# Luo ja näytä ponnahdusikkuna

- Toiminnallinen kutsu ponnahdusikkunakerroksen luomiseen ja näyttämiseen
- Poista elementit automaattisesti DOM:sta, kun se suljetaan
- Estä Esc-näppäimen peruutuskäyttäytyminen

## Käytä demoa

```html
<button>Napsauta avataksesi ponnahdusikkunan</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Otsikko";
  description.textContent = "Teksti";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Käyttöliittymän parametrit

### Viedyt oletustoiminnot

`Box()`

- **Palautusarvo**: `HTMLDialogElement`, tyyliluokan `Box` dialogielementti on lisätty.

## tyyliluokka

### `.Box`

Koskee `dialog`-elementtiä sijainnin, keskityksen ja taustamaskin sumennuksen määrittämiseksi.

### `.Lg`

Tyyliluokka, jota sovelletaan sisäiseen sisältöön, jossa asetetaan taustaväri, pyöristetyt kulmat, varjo- ja pystyasettelu sekä sisäisten `h3` ja `p` asettelu.