# Креирајте и прикажите искачући слој

- Функционални позив за креирање и приказ попуп слоја
- Аутоматски уклоните елементе из ДОМ-а када је затворен
- Спречите понашање отказивања тастера Есц

## Користите демо

```html
<button>Кликните да бисте искочили</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  титле.тектЦонтент = "Наслов";
  десцриптион.тектЦонтент = "Текст";
  button.className = "Btn Main";
  буттон.тектЦонтент = "ОК";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Параметри интерфејса

### Подразумеване извезене функције

`Box()`

- **Повратна вредност**: <б ид="1">, додат је елемент дијалога класе стила <б ид="2">.

## стилска класа

### `.Box`

Примењује се на елемент <б ид="1"> за подешавање позиционирања, центрирања и замућења маске позадине.

### `.Lg`

Класа стила примењена на унутрашњи садржај, која поставља боју позадине, заобљене углове, сенку и вертикални изглед и дефинише изглед унутрашњег <б ид="1"> и <б ид="2"> .