# ପପ୍ଅପ୍ ସ୍ତର ସୃଷ୍ଟି ଏବଂ ପ୍ରଦର୍ଶନ କରନ୍ତୁ |

- ପପ୍ଅପ୍ ସ୍ତର ସୃଷ୍ଟି ଏବଂ ପ୍ରଦର୍ଶନ କରିବାକୁ କାର୍ଯ୍ୟକ୍ଷମ କଲ୍ |
- ବନ୍ଦ ହେବା ସମୟରେ ସ୍ୱୟଂଚାଳିତ ଭାବରେ DOM ରୁ ଉପାଦାନଗୁଡିକ ଅପସାରଣ କରନ୍ତୁ |
- Esc କି ବାତିଲ ଆଚରଣକୁ ପ୍ରତିରୋଧ କରନ୍ତୁ |

## ଡେମୋ ବ୍ୟବହାର କରନ୍ତୁ |

```html
<button>ପପ୍ ଅପ୍ କରିବାକୁ କ୍ଲିକ୍ କରନ୍ତୁ |</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "ଆଖ୍ୟା";
  description.textContent = "ପାଠ";
  button.className = "Btn Main";
  button.textContent = "ଠିକ ଅଛି";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## ଇଣ୍ଟରଫେସ୍ ପାରାମିଟରଗୁଡିକ

### ଡିଫଲ୍ଟ ରପ୍ତାନି କାର୍ଯ୍ୟଗୁଡ଼ିକ |

`Box()`

- ** ଫେରସ୍ତ ମୂଲ୍ୟ **: `HTMLDialogElement`, `Box` ଶ style ଳୀ ଶ୍ରେଣୀର ସଂଳାପ ଉପାଦାନ ଯୋଗ କରାଯାଇଛି |

## ଶ style ଳୀ ଶ୍ରେଣୀ

### `.Box`

ପୋଜିସନ୍, ସେଣ୍ଟରିଂ ଏବଂ ବ୍ୟାକଗ୍ରାଉଣ୍ଡ ମାସ୍କ ଅସ୍ପଷ୍ଟତା ସେଟ୍ କରିବାକୁ `dialog` ଉପାଦାନରେ ପ୍ରଯୁଜ୍ୟ |

### `.Lg`

ଆଭ୍ୟନ୍ତରୀଣ ବିଷୟବସ୍ତୁରେ ପ୍ରୟୋଗ କରାଯାଉଥିବା ଏକ ଶ style ଳୀ ଶ୍ରେଣୀ, ପୃଷ୍ଠଭୂମି ରଙ୍ଗ, ଗୋଲାକାର କୋଣ, ଛାୟା ଏବଂ ଭୂଲମ୍ବ ଲେଆଉଟ୍ ସେଟିଂ, ଏବଂ ଭିତରର `h3` ଏବଂ `p` ର ଲେଆଉଟ୍ ବ୍ୟାଖ୍ୟା କରେ |