# Karta ro'yxatini asinxron tarzda yuklang va ko'rsating

- **Asinxron yuklash**: Asinxron funksiya orqali maʼlumotlarni oling
- **Holatni ko'rsatish**: uchta holatni qo'llab-quvvatlaydi: yuklash, bo'sh ma'lumotlar va kartani ko'rsatish.
- **Moslashuvchan tartib**: kartalar qatorlarga joylashtirilgan

## Ko‘rsatma ishlatish

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Sarlavha" + i, "Ta'rif" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interfeys parametrlari

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asinxron ma'lumotlarni yuklash funksiyasi. Har bir pastki kalitning formati `[nom, kirish, havola]`.

## uslublar sinfi

- `.CardLi`: tashqi element
- `.Card`: karta elementi
- `.Ing`: Animatsiyalangan element yuklanmoqda