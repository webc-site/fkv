# Ստեղծեք և ցուցադրեք թռուցիկ շերտ

- Ֆունկցիոնալ զանգ՝ թռուցիկ շերտ ստեղծելու և ցուցադրելու համար
- Ավտոմատ կերպով հեռացնել տարրերը DOM-ից, երբ փակ է
- Կանխել Esc բանալին չեղարկելու պահվածքը

## Օգտագործեք ցուցադրությունը

```html
<button>Սեղմեք՝ բաց թողնելու համար</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = «Վերնագիր»;
  description.textContent = «Տեքստ»;
  button.className = "Btn Main";
  button.textContent = «OK»;
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Ինտերֆեյսի պարամետրեր

### Կանխադրված արտահանվող գործառույթները

`Box()`

- **Վերադարձի արժեքը**. `HTMLDialogElement`, `Box` ոճի դասի երկխոսության տարրն ավելացվել է:

## ոճի դաս

### `.Box`

Կիրառվում է `dialog` տարրի վրա՝ դիրքավորումը, կենտրոնացումը և ֆոնային դիմակի մշուշումը սահմանելու համար:

### `.Lg`

Ոճերի դասը կիրառվում է ներքին բովանդակության վրա՝ սահմանելով ֆոնի գույնը, կլորացված անկյունները, ստվերային և ուղղահայաց դասավորությունը և սահմանելով ներքին `h3` և `p` դասավորությունը: