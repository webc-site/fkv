# ჩვეულებრივი ღილაკი და მრგვალი ხატის ღილაკი

## კლასის სახელი

- ნორმალური ღილაკი`.Btn`
- ხატულა ღილაკი `.BtnC`
- ძირითადი მონიშვნის მდგომარეობა`.Btn.Main`

## გამოყენება

დაამატეთ კლასის სახელი ელემენტს:

```html
<!-- ნორმალური ღილაკი -->
<button class="Btn">დაადასტურეთ</button>

<!-- მთავარი ხაზგასმა -->
<button class="Btn Main">წარადგინოს</button>

<!-- ბმული ღილაკი -->
<a class="Btn" href="https://github.com">ბმული ღილაკი</a>
```

## ხატის სტილი

`BtnC`-ის ხატულა შეიძლება მორგებული იყოს `background-image`-ით:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```