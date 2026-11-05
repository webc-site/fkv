# Завантажувати та відображати список карток асинхронно

- **Асинхронне завантаження**: отримуйте дані через асинхронну функцію
- **Відображення стану**: підтримує три стани: завантаження, порожні дані та візуалізація картки.
- **Гнучкий макет**: картки розташовані в ряди

## Використовуйте демо

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Назва " + i, "Опис" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Параметри інтерфейсу

### CardLi(loader)

- **завантажувач**: `() => Promise<Array<[string, string, string]>>`, функція асинхронного завантаження даних. Формат кожного підрозділу: `[назва, вступ, посилання]`.

## клас стилю

- `.CardLi`: зовнішній елемент
- `.Card`: елемент картки
- `.Ing`: Завантаження анімованого елемента