# డ్రాప్-డౌన్ ఎంపిక పెట్టె మరియు బటన్ కలయిక

- ఎంపిక పెట్టె మరియు బటన్‌ను క్షితిజ సమాంతరంగా విభజించండి
- ఎంచుకున్న స్థితిని స్థానిక నిల్వకు స్వయంచాలకంగా సేవ్ చేయండి
- బటన్ క్లిక్‌లను అడ్డగించండి మరియు అనుకూల ఈవెంట్‌లను పంపండి

## డెమో ఉపయోగించండి

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ఎంపిక 1</option>
    <option value="option2">ఎంపిక 2</option>
  </c-select>
  <button>పరుగు</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ఆస్తి

- `key`: స్థానిక నిల్వ నిలకడ కోసం కీ పేరు ఉపయోగించబడుతుంది
- `value`: ప్రస్తుతం ఎంచుకున్న విలువ

## సంఘటన

- `submit`: బటన్‌ను క్లిక్ చేసినప్పుడు పంపబడుతుంది, `e.value` ప్రస్తుతం ఎంచుకున్న విలువను కలిగి ఉంటుంది