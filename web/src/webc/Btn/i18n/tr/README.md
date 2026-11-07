# Sıradan düğme ve yuvarlak simge düğmesi

## Sınıf adı

- Normal düğme`.Btn`
- simge düğmesi `.BtnC`
- Birincil vurgulama durumu`.Btn.Main`

## kullanmak

Öğeye bir sınıf adı ekleyin:

```html
<!-- Normal düğme -->
<button class="Btn">onaylamak</button>

<!-- ana vurgu -->
<button class="Btn Main">göndermek</button>

<!-- bağlantı düğmesi -->
<a class="Btn" href="https://github.com">bağlantı düğmesi</a>
```

## simge stili

`BtnC` simgesi `background-image` aracılığıyla özelleştirilebilir:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```