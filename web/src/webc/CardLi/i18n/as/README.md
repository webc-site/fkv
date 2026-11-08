# কাৰ্ড তালিকা এচিনক্ৰ'নাছভাৱে লোড আৰু প্ৰদৰ্শন কৰক

- **এচিংক্ৰ'নাছ লোডিং**: এটা এচিনক্ৰ'নাছ ফাংচনৰ জৰিয়তে তথ্য লাভ কৰক
- **অৱস্থা প্ৰদৰ্শন**: তিনিটা অৱস্থা সমৰ্থন কৰে: লোডিং, খালী তথ্য, আৰু কাৰ্ড ৰেণ্ডাৰ কৰা।
- **নমনীয় বিন্যাস**: কাৰ্ডসমূহ শাৰীত সজোৱা হয়

## ডেমো ব্যৱহাৰ কৰক

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["শিৰোনাম " + i, "বিৱৰণ " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## আন্তঃপৃষ্ঠ প্ৰাচলসমূহ

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, এচিনক্ৰ'নাছ ডাটা লোডিং ফাংচন। প্ৰতিটো উপকিৰ বিন্যাস হৈছে `[নাম, পৰিচয়, সংযোগ]` ।

## শৈলী শ্ৰেণী

- `.CardLi`: বাহিৰৰ উপাদান
- `.Card`: কাৰ্ড উপাদান
- `.Ing`: এনিমেটেড উপাদান লোড কৰা হৈছে