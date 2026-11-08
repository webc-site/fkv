# Unda na uonyeshe safu ibukizi

- Simu inayofanya kazi ili kuunda na kuonyesha safu ibukizi
- Ondoa vipengele kiotomatiki kutoka kwa DOM wakati imefungwa
- Zuia tabia ya kughairi ufunguo wa Esc

## Tumia onyesho

```html
<button>Bofya ili pop up</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Kichwa";
  description.textContent = "Nakala";
  button.className = "Btn Main";
  button.textContent = "Sawa";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Vigezo vya interface

### Chaguo-msingi za kukokotoa zilizohamishwa

`Box()`

- **Thamani ya kurejesha**: `HTMLDialogElement`, kipengee cha kidadisi cha `Box` darasa la mtindo kimeongezwa.

## darasa la mtindo

### `.Box`

Hutumika kwa `dialog` kipengee ili kuweka uwekaji, uwekaji katikati, na ukungu wa mandharinyuma ya barakoa.

### `.Lg`

Darasa la mtindo linalotumika kwa maudhui ya ndani, kuweka rangi ya usuli, pembe za mviringo, kivuli na mpangilio wima, na kubainisha mpangilio wa ndani `h3` na `p` .