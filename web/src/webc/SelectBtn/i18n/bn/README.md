# ড্রপ-ডাউন নির্বাচন বাক্স এবং বোতামের সমন্বয়

- নির্বাচন বাক্স এবং বোতামটি অনুভূমিকভাবে স্প্লাইস করুন
- স্বয়ংক্রিয়ভাবে নির্বাচিত অবস্থা স্থানীয় স্টোরেজ এ সংরক্ষণ করুন
- ইন্টারসেপ্ট বোতাম ক্লিক এবং কাস্টম ঘটনা প্রেরণ

## ডেমো ব্যবহার করুন

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">বিকল্প 1</option>
    <option value="option2">বিকল্প 2</option>
  </c-select>
  <button>চালানো</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## সম্পত্তি

- `key`: স্থানীয় স্টোরেজ অধ্যবসায়ের জন্য ব্যবহৃত কী নাম
- `value`: বর্তমানে নির্বাচিত মান

## ঘটনা

- `submit`: বোতামটি ক্লিক করলে পাঠানো হয়, `e.value` বর্তমানে নির্বাচিত মান বহন করে