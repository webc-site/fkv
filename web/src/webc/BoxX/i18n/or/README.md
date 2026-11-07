# ଏକ ବନ୍ଦ ବଟନ୍ ସହିତ ଏକ ପପ୍ଅପ୍ ସ୍ତର ସୃଷ୍ଟି ଏବଂ ପ୍ରଦର୍ଶନ କରନ୍ତୁ |

- ପପ୍ଅପ୍ ସ୍ତର ସୃଷ୍ଟି ଏବଂ ପ୍ରଦର୍ଶନ କରିବାକୁ କାର୍ଯ୍ୟକ୍ଷମ କଲ୍ |
- ବିଲ୍ଟ-ଇନ୍ କ୍ଲୋଜ୍ ବଟନ୍, DOM ରୁ ଉପାଦାନକୁ ବନ୍ଦ ଏବଂ ଅପସାରଣ କରିବାକୁ କ୍ଲିକ୍ କରନ୍ତୁ |
- ଇନପୁଟ୍ ବାକ୍ସକୁ ଡିଫୋକସ୍ କରିବାକୁ କିମ୍ବା ପପ୍ଅପ୍ ସ୍ତରକୁ ବନ୍ଦ କରିବାକୁ Esc ଦବାନ୍ତୁ |

## ଡେମୋ ବ୍ୟବହାର କରନ୍ତୁ |

```html
<button>ପପ୍ ଅପ୍ କରିବାକୁ କ୍ଲିକ୍ କରନ୍ତୁ |</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ପାଠ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ଇଣ୍ଟରଫେସ୍ ପାରାମିଟରଗୁଡିକ

ଡିଫଲ୍ଟ ରପ୍ତାନି କାର୍ଯ୍ୟ `HTMLDialogElement` ରିଟର୍ନ କରେ, ଏବଂ `Box` ଏବଂ `X` ଶ style ଳୀ ଶ୍ରେଣୀଗୁଡ଼ିକ ସଂଳାପ ଉପାଦାନରେ ଯୋଗ କରାଯାଇଛି |

## ଶ style ଳୀ ଶ୍ରେଣୀ

### `.Box.X`

`dialog` ଉପାଦାନରେ ପ୍ରୟୋଗ, ସଂଳାପ ଶ style ଳୀ ନିର୍ଦ୍ଦିଷ୍ଟ କରେ |

### `a.X`

`a` ଉପାଦାନରେ ପ୍ରୟୋଗ, ବନ୍ଦ ବଟନ୍ ଶ style ଳୀ ନିର୍ଦ୍ଦିଷ୍ଟ କରେ |