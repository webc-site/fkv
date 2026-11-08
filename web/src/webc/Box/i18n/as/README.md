# পপআপ স্তৰ সৃষ্টি আৰু প্ৰদৰ্শন কৰক

- পপআপ স্তৰ সৃষ্টি আৰু প্ৰদৰ্শন কৰিবলৈ কাৰ্য্যকৰী কল
- বন্ধ কৰাৰ সময়ত DOM ৰ পৰা উপাদানসমূহ স্বয়ংক্ৰিয়ভাৱে আঁতৰাওক
- Esc চাবি বাতিল আচৰণ প্ৰতিৰোধ কৰক

## ডেমো ব্যৱহাৰ কৰক

```html
<button>পপ আপ কৰিবলৈ ক্লিক কৰক</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "শিৰোনাম";
  description.textContent = "পাঠ্য";
  button.className = "Btn Main";
  button.textContent = "ঠিক আছে";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## আন্তঃপৃষ্ঠ প্ৰাচলসমূহ

### অবিকল্পিত ৰপ্তানি কৰা ফলনসমূহ

`Box()`

- **মান ঘূৰাই দিয়ক**: `HTMLDialogElement`, `Box` শৈলী শ্ৰেণীৰ সংলাপ উপাদান যোগ কৰা হৈছে।

## শৈলী শ্ৰেণী

### `.Box`

অৱস্থান, কেন্দ্ৰীকৰণ, আৰু পটভূমি মাস্ক ব্লাৰ সংহতি কৰিবলে `dialog` উপাদানৰ বাবে প্ৰযোজ্য।

### `.Lg`

এটা শৈলী শ্ৰেণী ভিতৰৰ বিষয়বস্তুত প্ৰয়োগ কৰা হয়, পটভূমিৰ ৰং, ঘূৰণীয়া চুক, ছাঁ আৰু উলম্ব পৰিকল্পনা সংহতি কৰা, আৰু ভিতৰৰ `h3` আৰু `p` বিন্যাস সংজ্ঞায়িত কৰা।