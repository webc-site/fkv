# عام بٽڻ ۽ گول آئڪن بٽڻ

## ڪلاس جو نالو

- عام بٽڻ`.Btn`
- آئڪن بٽڻ `.BtnC`
- پرائمري نمايان حالت`.Btn.Main`

## استعمال ڪرڻ

عنصر ۾ ڪلاس جو نالو شامل ڪريو:

```html
<!-- عام بٽڻ -->
<button class="Btn">تصديق ڪرڻ</button>

<!-- مکيه نمايان -->
<button class="Btn Main">جمع ڪرائڻ</button>

<!-- لنڪ بٽڻ -->
<a class="Btn" href="https://github.com">لنڪ بٽڻ</a>
```

## آئڪن جو انداز

لاءِ آئڪن `BtnC` ترتيب ڏئي سگھجي ٿو `background-image` ذريعي:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```