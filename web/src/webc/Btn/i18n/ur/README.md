# عام بٹن اور گول آئیکن بٹن

## کلاس کا نام

- عام بٹن`.Btn`
- آئیکن بٹن `.BtnC`
- بنیادی ہائی لائٹ حالت`.Btn.Main`

## استعمال کریں

عنصر میں کلاس کا نام شامل کریں:

```html
<!-- عام بٹن -->
<button class="Btn">تصدیق کریں</button>

<!-- اہم نمایاں -->
<button class="Btn Main">جمع کروائیں</button>

<!-- لنک بٹن -->
<a class="Btn" href="https://github.com">لنک بٹن</a>
```

## آئیکن سٹائل

`BtnC` کے لیے آئیکن کو `background-image` کے ذریعے اپنی مرضی کے مطابق بنایا جا سکتا ہے:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```