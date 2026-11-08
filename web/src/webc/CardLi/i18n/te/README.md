# కార్డ్ జాబితాను అసమకాలికంగా లోడ్ చేయండి మరియు ప్రదర్శించండి

- ** అసమకాలిక లోడింగ్**: అసమకాలిక ఫంక్షన్ ద్వారా డేటాను పొందండి
- **స్టేటస్ డిస్‌ప్లే**: మూడు స్టేట్‌లకు మద్దతు ఇస్తుంది: లోడ్ చేయడం, ఖాళీ డేటా మరియు రెండరింగ్ కార్డ్.
- **ఫ్లెక్సిబుల్ లేఅవుట్**: కార్డ్‌లు వరుసలలో అమర్చబడి ఉంటాయి

## డెమో ఉపయోగించండి

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["శీర్షిక " + i, "వివరణ" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ఇంటర్ఫేస్ పారామితులు

### CardLi(loader)

- **లోడర్**: `() => Promise<Array<[string, string, string]>>`, అసమకాలిక డేటా లోడింగ్ ఫంక్షన్. ప్రతి సబ్‌కీ ఫార్మాట్ `[పేరు, పరిచయం, లింక్]`.

## శైలి తరగతి

- `.CardLi`: బాహ్య మూలకం
- `.Card`: కార్డ్ మూలకం
- `.Ing`: యానిమేటెడ్ మూలకం లోడ్ అవుతోంది