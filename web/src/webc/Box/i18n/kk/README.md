# Қалқымалы қабат жасаңыз және көрсетіңіз

- Қалқымалы қабатты құру және көрсету үшін функционалды шақыру
- Жабылған кезде DOM-дан элементтерді автоматты түрде алып тастаңыз
- Esc пернесін жою әрекетін болдырмау

## Демонстрацияны пайдаланыңыз

```html
<button>Қалқымалы шығу үшін басыңыз</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Тақырып";
  description.textContent = "Мәтін";
  button.className = "Btn Main";
  button.textContent = "Жарайды";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Интерфейс параметрлері

### Әдепкі экспортталған функциялар

`Box()`

- **Қайтарылатын мән**: `HTMLDialogElement`, `Box` стиль сыныбының диалогтық элементі қосылды.

## стиль сыныбы

### `.Box`

Орналасуды, орталықтандыруды және өң маскасының бұлдырлығын орнату үшін `dialog` элементіне қолданылады.

### `.Lg`

Ішкі мазмұнға қолданылатын стиль сыныбы, өң түсін, дөңгелектелген бұрыштарды, көлеңке мен тік орналасуды орнату және ішкі `h3` және `p` орналасуын анықтау.