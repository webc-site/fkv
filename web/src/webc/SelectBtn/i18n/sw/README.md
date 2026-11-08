# Mchanganyiko wa kisanduku cha uteuzi kunjuzi na kitufe

- Gawanya kisanduku cha uteuzi na kitufe kwa usawa
- Hifadhi kiotomatiki hali iliyochaguliwa kwenye Hifadhi ya ndani
- Mibofyo ya vitufe vya kukatiza na kutuma matukio maalum

## Tumia onyesho

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Chaguo 1</option>
    <option value="option2">Chaguo la 2</option>
  </c-select>
  <button>kukimbia</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## mali

- `key`: jina kuu linalotumika kwa uendelevu wa Hifadhi ya ndani
- `value`: thamani iliyochaguliwa kwa sasa

## tukio

- `submit`: Imetumwa wakati kitufe kinapobofya, `e.value` hubeba thamani iliyochaguliwa kwa sasa