# ផ្ទុក និងបង្ហាញបញ្ជីកាតដោយអសមកាល

- **ការផ្ទុកអសមកាល**៖ ទទួលបានទិន្នន័យតាមរយៈមុខងារអសមកាល
- ** ការបង្ហាញស្ថានភាព **៖ គាំទ្រស្ថានភាពបី៖ ការផ្ទុកទិន្នន័យទទេ និងកាតបង្ហាញ។
- **ប្លង់ដែលអាចបត់បែនបាន**៖ កាតត្រូវបានរៀបចំជាជួរ

## ប្រើការបង្ហាញ

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["ចំណងជើង " + i, "ការពិពណ៌នា " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ប៉ារ៉ាម៉ែត្រចំណុចប្រទាក់

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>` មុខងារផ្ទុកទិន្នន័យអសមកាល។ ទម្រង់នៃកូនសោរងនីមួយៗគឺ `[ឈ្មោះ ការណែនាំ តំណ]`។

## ថ្នាក់រចនាប័ទ្ម

- `.CardLi`៖ ធាតុខាងក្រៅ
- `.Card`៖ ធាតុកាត
- `.Ing`៖ កំពុងផ្ទុកធាតុមានចលនា