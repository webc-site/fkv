# دکمه معمولی و دکمه آیکون گرد

## نام کلاس

- دکمه معمولی`.Btn`
- دکمه نماد `.BtnC`
- حالت برجسته اولیه`.Btn.Main`

## استفاده کنید

یک نام کلاس به عنصر اضافه کنید:

```html
<!-- دکمه معمولی -->
<button class="Btn">تایید کنید</button>

<!-- برجسته اصلی -->
<button class="Btn Main">ارسال کنید</button>

<!-- دکمه پیوند -->
<a class="Btn" href="https://github.com">دکمه پیوند</a>
```

## سبک آیکون

نماد `BtnC` را می توان از طریق `background-image` سفارشی کرد:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```