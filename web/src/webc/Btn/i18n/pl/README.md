# Zwykły przycisk i okrągły przycisk z ikoną

## Nazwa klasy

- Normalny przycisk`.Btn`
- ikona przycisku `.BtnC`
- Podstawowy stan podświetlenia`.Btn.Main`

## używać

Dodaj nazwę klasy do elementu:

```html
<!-- Normalny przycisk -->
<button class="Btn">potwierdzać</button>

<!-- główna atrakcja -->
<button class="Btn Main">składać</button>

<!-- przycisk łącza -->
<a class="Btn" href="https://github.com">przycisk łącza</a>
```

## styl ikony

Ikonę `BtnC` można dostosować za pomocą `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```