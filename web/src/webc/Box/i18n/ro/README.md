# Creați și afișați stratul pop-up

- Apel funcțional pentru a crea și afișa stratul pop-up
- Eliminați automat elementele din DOM când este închis
- Preveniți comportamentul de anulare a tastei Esc

## Utilizați demonstrația

```html
<button>Faceți clic pentru a apărea</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = „Titlu”;
  description.textContent = „Text”;
  button.className = "Btn Main";
  button.textContent = „OK”;
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametrii interfeței

### Funcții implicite exportate

`Box()`

- **Valoare returnată**: `HTMLDialogElement`, elementul de dialog al clasei de stil `Box` a fost adăugat.

## clasa de stil

### `.Box`

Se aplică elementului `dialog` pentru a seta poziționarea, centrarea și estomparea măștii de fundal.

### `.Lg`

O clasă de stil aplicată conținutului interior, setând culoarea de fundal, colțurile rotunjite, umbra și aspectul vertical și definirea aspectului interiorului `h3` și `p` .