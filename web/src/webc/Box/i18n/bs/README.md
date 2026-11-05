# Kreirajte i prikažite popup sloj

- Funkcionalni poziv za kreiranje i prikaz popup sloja
- Automatski uklonite elemente iz DOM-a kada je zatvoren
- Spriječite ponašanje otkazivanja tipke Esc

## Koristite demo

```html
<button>Kliknite da se pojavi</button>

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

## Parametri interfejsa

### Zadane izvezene funkcije

`Box()`

- **Povratna vrijednost**: `HTMLDialogElement`, dodat je element dijaloga klase stila `Box`.

## stilska klasa

### `.Box`

Primjenjuje se na element `dialog` za postavljanje pozicioniranja, centriranja i zamućenja maske pozadine.

### `.Lg`

Klasa stila primijenjena na unutrašnji sadržaj, postavljajući boju pozadine, zaobljene uglove, sjenu i vertikalni izgled i definirajući izgled unutrašnjeg `h3` i `p` .