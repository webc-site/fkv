# لوڊ ڪريو ۽ ڏيکاريو ڪارڊ لسٽ غير مطابقت سان

- **Asynchronous loading**: ڊيٽا حاصل ڪريو هڪ asynchronous function ذريعي
- ** اسٽيٽس ڊسپلي**: ٽن رياستن کي سپورٽ ڪري ٿو: لوڊ ڪرڻ، خالي ڊيٽا، ۽ رينڊرنگ ڪارڊ.
- ** لچڪدار ترتيب**: ڪارڊ قطار ۾ ترتيب ڏنل آهن

## ڊيمو استعمال ڪريو

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["عنوان" + i، "تفصيل" + i، "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## انٽرفيس پيٽرول

### CardLi(loader)

- **لوڊر**: `() => Promise<Array<[string, string, string]>>`، غير مطابقت واري ڊيٽا لوڊ ڪرڻ وارو فنڪشن. هر سبڪي جي فارميٽ آهي `[نالو، تعارف، لنڪ]`.

## انداز ڪلاس

- `.CardLi`: ٻاهرين عنصر
- `.Card`: ڪارڊ عنصر
- `.Ing`: لوڊ ٿي رهيو آهي متحرڪ عنصر