# لیست کارت را به صورت ناهمزمان بارگیری و نمایش دهید

- **بارگیری ناهمزمان**: داده ها را از طریق یک تابع ناهمزمان دریافت کنید
- **نمایش وضعیت**: از سه حالت پشتیبانی می کند: بارگیری، داده خالی و کارت رندر.
- ** چیدمان انعطاف پذیر **: کارت ها در ردیف ها مرتب شده اند

## از دمو استفاده کنید

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["عنوان" + i، "توضیحات" + i، "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## پارامترهای رابط

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`، تابع بارگذاری ناهمزمان داده. قالب هر کلید فرعی `[نام، مقدمه، پیوند]` است.

## کلاس سبک

- `.CardLi`: عنصر بیرونی
- `.Card`: عنصر کارت
- `.Ing`: در حال بارگیری عنصر متحرک