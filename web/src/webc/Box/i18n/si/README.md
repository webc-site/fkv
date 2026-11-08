# උත්පතන ස්තරය සාදා සංදර්ශන කරන්න

- උත්පතන ස්තරය සෑදීමට සහ ප්‍රදර්ශනය කිරීමට ක්‍රියාකාරී ඇමතුම
- වසා ඇති විට DOM වෙතින් මූලද්‍රව්‍ය ස්වයංක්‍රීයව ඉවත් කරන්න
- Esc යතුරු අවලංගු කිරීමේ හැසිරීම වැළැක්වීම

## Demo භාවිතා කරන්න

```html
<button>උත්පතන කිරීමට ක්ලික් කරන්න</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "මාතෘකාව";
  description.textContent = "පෙළ";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## අතුරුමුහුණත් පරාමිතීන්

### පෙරනිමි අපනයනය කළ කාර්යයන්

`Box()`

- **ආපසු ලැබෙන අගය**: `HTMLDialogElement`, `Box` style class හි සංවාද මූලද්‍රව්‍යය එක් කර ඇත.

## ශෛලිය පන්තිය

### `.Box`

ස්ථානගත කිරීම, මධ්‍යගත කිරීම සහ පසුබිම් ආවරණ බොඳ කිරීම සැකසීමට `dialog` මූලද්‍රව්‍යයට අදාළ වේ.

### `.Lg`

පසුබිම් වර්ණය, වටකුරු කොන, සෙවනැලි සහ සිරස් පිරිසැලසුම සැකසීම සහ අභ්‍යන්තර `h3` සහ `p` හි පිරිසැලසුම නිර්වචනය කරමින් අභ්‍යන්තර අන්තර්ගතයට යොදන ලද විලාසිතා පන්තියකි.