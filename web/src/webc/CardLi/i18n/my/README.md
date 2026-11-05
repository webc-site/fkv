# ကတ်စာရင်းကို တပြိုင်တည်းတင်ပြီး ပြသပါ။

- **Asynchronous loading**- asynchronous function ဖြင့် ဒေတာကို ရယူပါ။
- **အခြေအနေပြသမှု**- အဆင့်သုံးဆင့်ကို ပံ့ပိုးပေးသည်- ဖွင့်ခြင်း၊ ဒေတာဗလာနှင့် ပုံဖော်ခြင်းကတ်။
- **Flexible Layout**- ကတ်များကို အတန်းလိုက်စီစဉ်ထားသည်။

## ဒီမိုကိုသုံးပါ။

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["ခေါင်းစဉ် " + i၊ "ဖော်ပြချက် " + i၊ "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## မျက်နှာပြင်ဘောင်များ

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`၊ အညီအမျှ ဒေတာတင်ခြင်း လုပ်ဆောင်ချက်။ ကီးတစ်ခုစီ၏ ဖော်မတ်သည် `[အမည်၊ မိတ်ဆက်၊ လင့်ခ်]` ဖြစ်သည်။

## စတိုင်အတန်း

- `.CardLi`- ပြင်ပဒြပ်စင်
- `.Card`- ကတ်ဒြပ်စင်
- `.Ing`- ကာတွန်းဒြပ်စင်ကို တင်နေသည်။