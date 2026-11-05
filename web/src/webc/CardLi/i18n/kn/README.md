# ಕಾರ್ಡ್ ಪಟ್ಟಿಯನ್ನು ಅಸಮಕಾಲಿಕವಾಗಿ ಲೋಡ್ ಮಾಡಿ ಮತ್ತು ಪ್ರದರ್ಶಿಸಿ

- **ಅಸಿಂಕ್ರೊನಸ್ ಲೋಡಿಂಗ್**: ಅಸಮಕಾಲಿಕ ಕ್ರಿಯೆಯ ಮೂಲಕ ಡೇಟಾವನ್ನು ಪಡೆಯಿರಿ
- ** ಸ್ಥಿತಿ ಪ್ರದರ್ಶನ **: ಮೂರು ಸ್ಥಿತಿಗಳನ್ನು ಬೆಂಬಲಿಸುತ್ತದೆ: ಲೋಡ್ ಮಾಡುವುದು, ಖಾಲಿ ಡೇಟಾ ಮತ್ತು ರೆಂಡರಿಂಗ್ ಕಾರ್ಡ್.
- ** ಹೊಂದಿಕೊಳ್ಳುವ ಲೇಔಟ್**: ಕಾರ್ಡ್‌ಗಳನ್ನು ಸಾಲುಗಳಲ್ಲಿ ಜೋಡಿಸಲಾಗಿದೆ

## ಡೆಮೊ ಬಳಸಿ

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["ಶೀರ್ಷಿಕೆ " + i, "ವಿವರಣೆ" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ಇಂಟರ್ಫೇಸ್ ನಿಯತಾಂಕಗಳು

### CardLi(loader)

- **ಲೋಡರ್**: `() => Promise<Array<[string, string, string]>>`, ಅಸಮಕಾಲಿಕ ಡೇಟಾ ಲೋಡಿಂಗ್ ಕಾರ್ಯ. ಪ್ರತಿ ಉಪಕೀಯ ಸ್ವರೂಪವು `[ಹೆಸರು, ಪರಿಚಯ, ಲಿಂಕ್]` ಆಗಿದೆ.

## ಶೈಲಿ ವರ್ಗ

- `.CardLi`: ಹೊರಗಿನ ಅಂಶ
- `.Card`: ಕಾರ್ಡ್ ಅಂಶ
- `.Ing`: ಅನಿಮೇಟೆಡ್ ಅಂಶವನ್ನು ಲೋಡ್ ಮಾಡಲಾಗುತ್ತಿದೆ