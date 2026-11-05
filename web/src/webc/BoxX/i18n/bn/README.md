# একটি বন্ধ বোতাম দিয়ে একটি পপআপ স্তর তৈরি করুন এবং প্রদর্শন করুন৷

- পপআপ স্তর তৈরি এবং প্রদর্শনের জন্য কার্যকরী কল
- অন্তর্নির্মিত বন্ধ বোতাম, বন্ধ করতে ক্লিক করুন এবং DOM থেকে উপাদানটি সরান৷
- ইনপুট বক্স ডিফোকাস করতে বা পপআপ স্তর বন্ধ করতে Esc টিপুন

## ডেমো ব্যবহার করুন

```html
<button>পপ আপ করতে ক্লিক করুন</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "টেক্সট";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ইন্টারফেস পরামিতি

ডিফল্ট এক্সপোর্ট ফাংশন `HTMLDialogElement` প্রদান করে, এবং `Box` এবং `X` স্টাইল ক্লাস ডায়ালগ উপাদানে যোগ করা হয়েছে।

## শৈলী ক্লাস

### `.Box.X`

`dialog` উপাদানে প্রয়োগ করা হয়, ডায়ালগ শৈলী নির্দিষ্ট করে।

### `a.X`

`a` উপাদানে প্রয়োগ করা হয়েছে, বন্ধ বোতাম শৈলী নির্দিষ্ট করে।