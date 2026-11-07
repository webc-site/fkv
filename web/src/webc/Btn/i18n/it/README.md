# Pulsante ordinario e pulsante con icona rotonda

## Nome della classe

- Pulsante normale`.Btn`
- pulsante icona `.BtnC`
- Stato di evidenziazione principale`.Btn.Main`

## utilizzo

Aggiungi un nome di classe all'elemento:

```html
<!-- Pulsante normale -->
<button class="Btn">confermare</button>

<!-- clou principale -->
<button class="Btn Main">invia</button>

<!-- pulsante di collegamento -->
<a class="Btn" href="https://github.com">pulsante di collegamento</a>
```

## stile dell'icona

L'icona per `BtnC` può essere personalizzata tramite `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```