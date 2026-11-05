# කාඩ්පත් ලැයිස්තුව අසමමිතිකව පූරණය කර ප්‍රදර්ශනය කරන්න

- **Asynchronous loading**: Asynchronous function හරහා දත්ත ලබාගන්න
- **තත්ත්ව සංදර්ශකය**: ප්‍රාන්ත තුනකට සහය දක්වයි: පැටවීම, හිස් දත්ත සහ විදැහුම් කාඩ්පත.
- ** නම්‍යශීලී පිරිසැලසුම**: කාඩ්පත් පේළි වලින් සකසා ඇත

## Demo භාවිතා කරන්න

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["මාතෘකාව " + i, "විස්තරය" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## අතුරුමුහුණත් පරාමිතීන්

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, අසමමුහුර්ත දත්ත පැටවීමේ කාර්යය. එක් එක් උප යතුරෙහි ආකෘතිය `[නම, හැඳින්වීම, සබැඳිය]` වේ.

## ශෛලිය පන්තිය

- `.CardLi`: පිටත මූලද්‍රව්‍යය
- `.Card`: කාඩ්පත් මූලාංගය
- `.Ing`: සජීවිකරණ මූලද්‍රව්‍ය පූරණය කරමින්