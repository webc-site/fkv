# সাধাৰণ বুটাম আৰু ঘূৰণীয়া আইকন বুটাম

## শ্ৰেণীৰ নাম

- সাধাৰণ বুটাম`.Btn`
- আইকন বুটাম `.BtnC`
- প্ৰাথমিক হাইলাইট অৱস্থা`.Btn.Main`

## ব্যৱহাৰ

উপাদানটোত এটা শ্ৰেণীৰ নাম যোগ কৰক:

```html
<!-- সাধাৰণ বুটাম -->
<button class="Btn">নিশ্চিত</button>

<!-- মূল হাইলাইট -->
<button class="Btn Main">দাখিল কৰক</button>

<!-- লিংক বুটাম -->
<a class="Btn" href="https://github.com">লিংক বুটাম</a>
```

## আইকন শৈলী

`BtnC` ৰ বাবে আইকন `background-image` ৰ যোগেদি স্বনিৰ্বাচিত কৰিব পাৰি:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```