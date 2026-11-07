# Navaden gumb in gumb z okroglo ikono

## Ime razreda

- Običajni gumb`.Btn`
- ikona gumb `.BtnC`
- Primarno poudarjeno stanje`.Btn.Main`

## uporaba

Elementu dodajte ime razreda:

```html
<!-- Običajni gumb -->
<button class="Btn">potrdite</button>

<!-- glavni poudarek -->
<button class="Btn Main">predložiti</button>

<!-- gumb za povezavo -->
<a class="Btn" href="https://github.com">gumb za povezavo</a>
```

## slog ikon

Ikono za `BtnC` lahko prilagodite prek `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```