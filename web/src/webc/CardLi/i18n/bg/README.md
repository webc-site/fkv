# Зареждане и показване на списък с карти асинхронно

- **Асинхронно зареждане**: Вземете данни чрез асинхронна функция
- **Дисплей на състоянието**: Поддържа три състояния: зареждане, празни данни и рендиране на карта.
- **Гъвкаво оформление**: Картите са подредени в редове

## Използвайте демонстрацията

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Заглавие " + i, "Описание " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Параметри на интерфейса

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, функция за асинхронно зареждане на данни. Форматът на всеки подключ е `[име, въведение, връзка]`.

## стил клас

- `.CardLi`: външен елемент
- `.Card`: елемент на картата
- `.Ing`: Зарежда се анимиран елемент