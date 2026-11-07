# تحميل وعرض قائمة البطاقات بشكل غير متزامن

- ** التحميل غير المتزامن **: احصل على البيانات من خلال وظيفة غير متزامنة
- **عرض الحالة**: يدعم ثلاث حالات: التحميل، والبيانات الفارغة، وبطاقة العرض.
- **تخطيط مرن**: يتم ترتيب البطاقات في صفوف

## استخدم العرض التوضيحي

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["العنوان" + i, "الوصف" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## معلمات الواجهة

### CardLi(loader)

- **المحمل**: `() => Promise<Array<[string, string, string]>>`، وظيفة تحميل البيانات غير المتزامنة. تنسيق كل مفتاح فرعي هو `[الاسم، المقدمة، الرابط]`.

## فئة النمط

- `.CardLi`: العنصر الخارجي
- `.Card`: عنصر البطاقة
- `.Ing`: تحميل العنصر المتحرك