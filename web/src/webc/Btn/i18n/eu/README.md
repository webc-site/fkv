# Botoi arrunta eta ikono biribila

## Klasearen izena

- Botoi arrunta`.Btn`
- ikono-botoia `.BtnC`
- Nabarmentzeko egoera nagusia`.Btn.Main`

## erabili

Gehitu klase-izen bat elementuari:

```html
<!-- Botoi arrunta -->
<button class="Btn">berretsi</button>

<!-- aipagarri nagusia -->
<button class="Btn Main">aurkeztu</button>

<!-- esteka botoia -->
<a class="Btn" href="https://github.com">esteka botoia</a>
```

## ikonoaren estiloa

`BtnC`-ren ikonoa `background-image` bidez pertsonaliza daiteke:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```