# এটা বন্ধ বুটামৰ সৈতে এটা পপআপ স্তৰ সৃষ্টি আৰু প্ৰদৰ্শন কৰক

- পপআপ স্তৰ সৃষ্টি আৰু প্ৰদৰ্শন কৰিবলৈ কাৰ্য্যকৰী কল
- বিল্ট-ইন বন্ধ বুটাম, বন্ধ কৰিবলৈ আৰু DOM ৰ পৰা উপাদান আঁতৰাবলৈ ক্লিক কৰক
- ইনপুট বাকচ ডিফ'কাচ কৰিবলে Esc টিপক বা পপআপ স্তৰ বন্ধ কৰক

## ডেমো ব্যৱহাৰ কৰক

```html
<button>পপ আপ কৰিবলৈ ক্লিক কৰক</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "পাঠ্য";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## আন্তঃপৃষ্ঠ প্ৰাচলসমূহ

অবিকল্পিত ৰপ্তানি ফলনে `HTMLDialogElement` ঘূৰাই দিয়ে, আৰু `Box` আৰু `X` শৈলী শ্ৰেণীসমূহক সংলাপ উপাদানত যোগ কৰা হৈছে।

## শৈলী শ্ৰেণী

### `.Box.X`

`dialog` উপাদানত প্ৰয়োগ কৰা হৈছে, সংলাপ শৈলী ধাৰ্য্য কৰে ।

### `a.X`

`a` উপাদানত প্ৰয়োগ কৰা হৈছে, বন্ধ বুটাম শৈলী ধাৰ্য্য কৰে।