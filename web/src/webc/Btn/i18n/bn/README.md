# সাধারণ বোতাম এবং বৃত্তাকার আইকন বোতাম

## ক্লাসের নাম

- সাধারণ বোতাম`.Btn`৷
- আইকন বোতাম `.BtnC`
- প্রাথমিক হাইলাইট অবস্থা`.Btn.Main`৷

## ব্যবহার

উপাদানটিতে একটি শ্রেণীর নাম যোগ করুন:

```html
<!-- সাধারণ বোতাম -->
<button class="Btn">নিশ্চিত করুন</button>

<!-- প্রধান হাইলাইট -->
<button class="Btn Main">জমা</button>

<!-- লিঙ্ক বোতাম -->
<a class="Btn" href="https://github.com">লিঙ্ক বোতাম</a>
```

## আইকন শৈলী

`BtnC` এর আইকন `background-image` এর মাধ্যমে কাস্টমাইজ করা যেতে পারে:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```