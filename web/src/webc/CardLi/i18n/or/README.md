# ଅସନ୍ତୁଳିତ ଭାବରେ କାର୍ଡ ତାଲିକା ଲୋଡ୍ ଏବଂ ପ୍ରଦର୍ଶନ କରନ୍ତୁ |

- ** ଅସନ୍ତୁଳିତ ଲୋଡିଂ **: ଏକ ଅସନ୍ତୁଳିତ କାର୍ଯ୍ୟ ମାଧ୍ୟମରେ ତଥ୍ୟ ପ୍ରାପ୍ତ କରନ୍ତୁ |
- ** ସ୍ଥିତି ପ୍ରଦର୍ଶନ **: ତିନୋଟି ରାଜ୍ୟକୁ ସମର୍ଥନ କରେ: ଲୋଡିଂ, ଖାଲି ଡାଟା, ଏବଂ ରେଣ୍ଡରିଂ କାର୍ଡ |
- ** ଫ୍ଲେକ୍ସିବଲ୍ ଲେଆଉଟ୍ **: କାର୍ଡଗୁଡ଼ିକ ଧାଡିରେ ସଜାଯାଇଛି |

## ଡେମୋ ବ୍ୟବହାର କରନ୍ତୁ |

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push (["ଆଖ୍ୟା" + i, "ବର୍ଣ୍ଣନା" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## ଇଣ୍ଟରଫେସ୍ ପାରାମିଟରଗୁଡିକ

### CardLi(loader)

- ** ଲୋଡର୍ **: `() => Promise<Array<[string, string, string]>>`, ଅସନ୍ତୁଳିତ ଡାଟା ଲୋଡିଂ କାର୍ଯ୍ୟ | ପ୍ରତ୍ୟେକ ସବ୍କିର ଫର୍ମାଟ୍ ହେଉଛି <b ଶ୍ରେଣୀ = "କୋଡ୍"> [ନାମ, ପରିଚୟ, ଲିଙ୍କ୍] ` |

## ଶ style ଳୀ ଶ୍ରେଣୀ

- `.CardLi`: ବାହ୍ୟ ଉପାଦାନ |
- `.Card`: କାର୍ଡ ଉପାଦାନ |
- `.Ing`: ଆନିମେଟେଡ୍ ଉପାଦାନ ଲୋଡ୍ କରୁଛି |