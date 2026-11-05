# Stvorite i prikažite skočni sloj

- Funkcionalni poziv za stvaranje i prikaz skočnog sloja
- Automatski uklanja elemente iz DOM-a kada se zatvori
- Spriječite otkazivanje tipke Esc

## Koristite demo

```html
<button>Kliknite za iskakanje</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Naslov";
  description.textContent = "Tekst";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametri sučelja

### Zadane izvezene funkcije

`Box()`

- **Povratna vrijednost**: `HTMLDialogElement`, dodan je dijaloški element klase stila `Box`.

## stilska klasa

### `.Box`

Primjenjuje se na element `dialog` za postavljanje pozicioniranja, centriranja i zamućenja maske pozadine.

### `.Lg`

Klasa stila primijenjena na unutarnji sadržaj, postavljanje boje pozadine, zaobljenih kutova, sjene i okomitog rasporeda te definiranje izgleda unutarnjeg `h3` i `p` .