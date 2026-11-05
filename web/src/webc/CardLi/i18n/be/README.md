# Загрузка і адлюстраванне спісу карт асінхронна

- **Асінхронная загрузка**: Атрымлівайце даныя праз асінхронную функцыю
- **Адлюстраванне стану**: падтрымлівае тры станы: загрузка, пустыя дадзеныя і візуалізацыя карты.
- **Гнуткі макет**: карты размешчаны ў шэрагі

## Выкарыстоўвайце дэма

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Назва" + i, "Апісанне" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Параметры інтэрфейсу

### CardLi(loader)

- **загрузчык**: `() => Promise<Array<[string, string, string]>>`, функцыя асінхроннай загрузкі даных. Фармат кожнага падключа: `[імя, увядзенне, спасылка]`.

## клас стылю

- `.CardLi`: знешні элемент
- `.Card`: элемент карты
- `.Ing`: Загрузка аніміраванага элемента