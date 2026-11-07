# Előugró réteg létrehozása és megjelenítése

- Funkcionális hívás előugró réteg létrehozásához és megjelenítéséhez
- Bezáráskor automatikusan eltávolítja az elemeket a DOM-ból
- Az Esc billentyű törlésének megakadályozása

## Használd a demót

```html
<button>Kattintson a felbukkanáshoz</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Cím";
  description.textContent = "Szöveg";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interfész paraméterei

### Alapértelmezett exportált függvények

`Box()`

- **Visszatérési érték**: `HTMLDialogElement`, a `Box` stílusosztály párbeszédablak eleme hozzáadásra került.

## stílus osztály

### `.Box`

Az `dialog` elemre vonatkozik a pozicionálás, a központosítás és a háttérmaszk elmosódásának beállításához.

### `.Lg`

A belső tartalomra alkalmazott stílusosztály, amely beállítja a háttérszínt, a lekerekített sarkokat, az árnyékos és függőleges elrendezést, valamint meghatározza a belső `h3` és `p` elrendezését.