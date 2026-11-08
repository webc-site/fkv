# অ্যাসিঙ্ক্রোনাসভাবে কার্ড তালিকা লোড এবং প্রদর্শন করুন

- **অসিঙ্ক্রোনাস লোডিং**: একটি অ্যাসিঙ্ক্রোনাস ফাংশনের মাধ্যমে ডেটা পান
- **স্থিতি প্রদর্শন**: তিনটি অবস্থা সমর্থন করে: লোডিং, খালি ডেটা এবং রেন্ডারিং কার্ড।
- **নমনীয় বিন্যাস**: কার্ডগুলি সারিবদ্ধভাবে সাজানো হয়েছে

## ডেমো ব্যবহার করুন

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["শিরোনাম" + i, "বিবরণ " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ইন্টারফেস পরামিতি

### CardLi(loader)

- **লোডার**: `() => Promise<Array<[string, string, string]>>`, অ্যাসিঙ্ক্রোনাস ডেটা লোডিং ফাংশন। প্রতিটি সাবকির বিন্যাস হল `[নাম, ভূমিকা, লিঙ্ক]`।

## শৈলী ক্লাস

- `.CardLi`: বাইরের উপাদান
- `.Card`: কার্ডের উপাদান
- `.Ing`: অ্যানিমেটেড উপাদান লোড হচ্ছে