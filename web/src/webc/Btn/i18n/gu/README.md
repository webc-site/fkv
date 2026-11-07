# સામાન્ય બટન અને રાઉન્ડ આઇકન બટન

## વર્ગનું નામ

- સામાન્ય બટન`.Btn`
- આઇકોન બટન `.BtnC`
- પ્રાથમિક હાઇલાઇટ સ્થિતિ`.Btn.Main`

## ઉપયોગ

તત્વમાં વર્ગનું નામ ઉમેરો:

```html
<!-- સામાન્ય બટન -->
<button class="Btn">પુષ્ટિ કરો</button>

<!-- મુખ્ય હાઇલાઇટ -->
<button class="Btn Main">સબમિટ કરો</button>

<!-- લિંક બટન -->
<a class="Btn" href="https://github.com">લિંક બટન</a>
```

## ચિહ્ન શૈલી

`BtnC` માટેનું આયકન `background-image` દ્વારા કસ્ટમાઇઝ કરી શકાય છે:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```