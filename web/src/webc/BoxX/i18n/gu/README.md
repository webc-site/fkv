# બંધ બટન વડે પોપઅપ લેયર બનાવો અને પ્રદર્શિત કરો

- પોપઅપ લેયર બનાવવા અને પ્રદર્શિત કરવા માટે કાર્યાત્મક કૉલ
- બિલ્ટ-ઇન ક્લોઝ બટન, DOMમાંથી ઘટકને બંધ કરવા અને દૂર કરવા માટે ક્લિક કરો
- ઇનપુટ બોક્સને ડીફોકસ કરવા અથવા પોપઅપ લેયરને બંધ કરવા માટે Esc દબાવો

## ડેમોનો ઉપયોગ કરો

```html
<button>પોપ અપ કરવા માટે ક્લિક કરો</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ટેક્સ્ટ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ઇન્ટરફેસ પરિમાણો

ડિફૉલ્ટ નિકાસ કાર્ય `HTMLDialogElement` આપે છે, અને `Box` અને `X` શૈલીના વર્ગો સંવાદ ઘટકમાં ઉમેરવામાં આવ્યા છે.

## શૈલી વર્ગ

### `.Box.X`

`dialog` તત્વ પર લાગુ, સંવાદ શૈલીનો ઉલ્લેખ કરે છે.

### `a.X`

`a` તત્વ પર લાગુ, બંધ બટન શૈલીનો ઉલ્લેખ કરે છે.