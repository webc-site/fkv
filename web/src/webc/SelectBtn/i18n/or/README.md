# ଡ୍ରପ୍-ଡାଉନ୍ ଚୟନ ବାକ୍ସ ଏବଂ ବଟନ୍ ର ମିଶ୍ରଣ |

- ଭୂସମାନ୍ତର ଭାବରେ ଚୟନ ବାକ୍ସ ଏବଂ ବଟନ୍ କୁ ଭାଗ କରନ୍ତୁ |
- ସ୍ୱୟଂଚାଳିତ ଭାବରେ ମନୋନୀତ ସ୍ଥିତିକୁ ସ୍ଥାନୀୟ ଷ୍ଟୋରେଜ୍ ରେ ସେଭ୍ କରନ୍ତୁ |
- ବଟନ୍ କ୍ଲିକ୍ ଏବଂ କଷ୍ଟମ୍ ଇଭେଣ୍ଟ ପଠାନ୍ତୁ |

## ଡେମୋ ବ୍ୟବହାର କରନ୍ତୁ |

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ବିକଳ୍ପ 1</option>
    <option value="option2">ବିକଳ୍ପ 2</option>
  </c-select>
  <button>ଚଲାନ୍ତୁ |</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ସମ୍ପତ୍ତି

- `key`: ସ୍ଥାନୀୟ ଷ୍ଟୋରେଜ୍ ସ୍ଥିରତା ପାଇଁ ବ୍ୟବହୃତ ମୁଖ୍ୟ ନାମ |
- `value`: ସମ୍ପ୍ରତି ମନୋନୀତ ମୂଲ୍ୟ |

## ଇଭେଣ୍ଟ

- `submit`: ବଟନ୍ କ୍ଲିକ୍ ହେବାବେଳେ ପଠାଯାଏ, `e.value` ସମ୍ପ୍ରତି ମନୋନୀତ ମୂଲ୍ୟ ବହନ କରେ |