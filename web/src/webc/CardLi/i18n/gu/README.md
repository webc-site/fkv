# અસુમેળ રીતે કાર્ડ સૂચિ લોડ કરો અને પ્રદર્શિત કરો

- **અસુમેળ લોડિંગ**: અસુમેળ કાર્ય દ્વારા ડેટા મેળવો
- **સ્ટેટસ ડિસ્પ્લે**: ત્રણ સ્ટેટ્સને સપોર્ટ કરે છે: લોડિંગ, ખાલી ડેટા અને રેન્ડરિંગ કાર્ડ.
- **ફ્લેક્સિબલ લેઆઉટ**: કાર્ડ્સ પંક્તિઓમાં ગોઠવાયેલા છે

## ડેમોનો ઉપયોગ કરો

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["શીર્ષક " + i, "વર્ણન " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ઇન્ટરફેસ પરિમાણો

### CardLi(loader)

- **લોડર**: `() => Promise<Array<[string, string, string]>>`, અસુમેળ ડેટા લોડિંગ કાર્ય. દરેક સબકીનું ફોર્મેટ `[નામ, પરિચય, લિંક]` છે.

## શૈલી વર્ગ

- `.CardLi`: બાહ્ય તત્વ
- `.Card`: કાર્ડ એલિમેન્ટ
- `.Ing`: એનિમેટેડ ઘટક લોડ કરી રહ્યું છે