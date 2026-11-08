# পপআপ স্তর তৈরি করুন এবং প্রদর্শন করুন

- পপআপ স্তর তৈরি এবং প্রদর্শনের জন্য কার্যকরী কল
- বন্ধ হয়ে গেলে স্বয়ংক্রিয়ভাবে DOM থেকে উপাদানগুলি সরান৷
- Esc কী বাতিলকরণ আচরণ প্রতিরোধ করুন

## ডেমো ব্যবহার করুন

```html
<button>পপ আপ করতে ক্লিক করুন</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "শিরোনাম";
  description.textContent = "টেক্সট";
  button.className = "Btn Main";
  button.textContent = "ঠিক আছে";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ইন্টারফেস পরামিতি

### ডিফল্ট এক্সপোর্ট ফাংশন

`Box()`

- **রিটার্ন মান**: `HTMLDialogElement`, `Box` স্টাইল ক্লাসের ডায়ালগ উপাদান যোগ করা হয়েছে।

## শৈলী ক্লাস

### `.Box`

পজিশনিং, সেন্টারিং, এবং ব্যাকগ্রাউন্ড মাস্ক ব্লার সেট করতে `dialog` এলিমেন্টে প্রযোজ্য।

### `.Lg`

পটভূমির রঙ, বৃত্তাকার কোণ, ছায়া এবং উল্লম্ব বিন্যাস সেট করে এবং ভিতরের `h3` এবং `p` এর বিন্যাস সংজ্ঞায়িত করে অভ্যন্তরীণ সামগ্রীতে একটি শৈলী শ্রেণী প্রয়োগ করা হয়।