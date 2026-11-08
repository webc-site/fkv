# Загрузка и отображение списка карточек асинхронно

- **Асинхронная загрузка**: получение данных с помощью асинхронной функции.
- **Отображение состояния**: поддерживает три состояния: загрузка, пустые данные и отображение карты.
- **Гибкий макет**: карточки располагаются рядами.

## Используйте демо-версию

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Название" + i, "Описание" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Параметры интерфейса

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, функция асинхронной загрузки данных. Формат каждого подраздела: `[имя, введение, ссылка]`.

## класс стиля

- `.CardLi`: внешний элемент
- `.Card`: элемент карты
- `.Ing`: загрузка анимированного элемента.