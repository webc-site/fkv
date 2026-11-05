# زر عادي وزر أيقونة مستديرة

## اسم الفئة

- الزر العادي `.Btn`
- زر الرمز `.BtnC`
- حالة التمييز الأساسية`.Btn.Main`

## يستخدم

أضف اسم فئة إلى العنصر:

```html
<!-- زر عادي -->
<button class="Btn">يتأكد</button>

<!-- تسليط الضوء الرئيسي -->
<button class="Btn Main">يُقدِّم</button>

<!-- زر الارتباط -->
<a class="Btn" href="https://github.com">زر الارتباط</a>
```

## نمط الأيقونة

يمكن تخصيص رمز `BtnC` عبر `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```