# Բեռնել և ցուցադրել քարտերի ցանկը ասինքրոն

- **Ասինխրոն բեռնում**. Ստացեք տվյալներ ասինխրոն ֆունկցիայի միջոցով
- **Կարգավիճակի ցուցադրում**. Աջակցում է երեք վիճակների` բեռնում, դատարկ տվյալներ և քարտի տրամադրում:
- **Ճկուն դասավորություն**. Քարտերը դասավորված են շարքերով

## Օգտագործեք ցուցադրությունը

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push (["Վերնագիր" + i, "Նկարագրություն" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Ինտերֆեյսի պարամետրեր

### CardLi(loader)

- **loader**. `() => Promise<Array<[string, string, string]>>`, ասինխրոն տվյալների բեռնման գործառույթ: Յուրաքանչյուր ենթաբանալու ձևաչափն է `[անուն, ներածություն, հղում]`:

## ոճի դաս

- `.CardLi`՝ արտաքին տարր
- `.Card`՝ քարտի տարր
- `.Ing`. անիմացիոն տարրի բեռնում