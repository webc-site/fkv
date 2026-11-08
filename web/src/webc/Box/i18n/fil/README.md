# Lumikha at magpakita ng popup layer

- Functional na tawag para gumawa at magpakita ng popup layer
- Awtomatikong alisin ang mga elemento mula sa DOM kapag sarado
- Pigilan ang gawi sa pagkansela ng Esc key

## Gamitin ang demo

```html
<button>I-click para mag-pop up</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Pamagat";
  description.textContent = "Text";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Mga parameter ng interface

### Default na na-export na mga function

`Box()`

- **Return value**: `HTMLDialogElement`, naidagdag na ang dialog element ng `Box` style class.

## klase ng istilo

### `.Box`

Nalalapat sa elementong `dialog` upang itakda ang pagpoposisyon, pagsentro, at pag-blur ng background mask.

### `.Lg`

Isang klase ng istilo na inilapat sa panloob na nilalaman, pagtatakda ng kulay ng background, mga bilugan na sulok, anino at patayong layout, at pagtukoy sa layout ng panloob na `h3` at `p` .