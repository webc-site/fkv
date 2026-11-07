# کارڈ کی فہرست کو متضاد طور پر لوڈ اور ڈسپلے کریں۔

- **اسینکرونس لوڈنگ**: ایک غیر مطابقت پذیر فنکشن کے ذریعے ڈیٹا حاصل کریں۔
- **اسٹیٹس ڈسپلے**: تین حالتوں کو سپورٹ کرتا ہے: لوڈنگ، خالی ڈیٹا، اور رینڈرنگ کارڈ۔
- **لچکدار لے آؤٹ**: کارڈز کو قطاروں میں ترتیب دیا گیا ہے۔

## ڈیمو استعمال کریں۔

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["عنوان" + i، "تفصیل" + i، "#" + i])؛
  }
  return data;
});

document.body.append(card_li);
</script>
```

## انٹرفیس کے پیرامیٹرز

### CardLi(loader)

- **لوڈر**: `() => Promise<Array<[string, string, string]>>`، غیر مطابقت پذیر ڈیٹا لوڈنگ فنکشن۔ ہر ذیلی کلید کا فارمیٹ `[نام، تعارف، لنک]` ہے۔

## سٹائل کلاس

- `.CardLi`: بیرونی عنصر
- `.Card`: کارڈ عنصر
- `.Ing`: متحرک عنصر لوڈ ہو رہا ہے۔