# පතන තේරීම් කොටුව සහ බොත්තම ඒකාබද්ධ කිරීම

- තේරීම් කොටුව සහ බොත්තම තිරස් අතට ස්ප්ලයිස් කරන්න
- තෝරාගත් තත්වය දේශීය ගබඩාවට ස්වයංක්‍රීයව සුරකින්න
- බොත්තම ක්ලික් කිරීම් බාධා කර අභිරුචි සිදුවීම් යැවීම

## Demo භාවිතා කරන්න

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">විකල්ප 1</option>
    <option value="option2">විකල්ප 2</option>
  </c-select>
  <button>දුවනවා</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## දේපල

- `key`: localStorage කල්පවත්නා සඳහා භාවිතා කරන ප්‍රධාන නම
- `value`: දැනට තෝරාගත් අගය

## සිදුවීම

- `submit`: බොත්තම ක්ලික් කළ විට යවනු ලැබේ, `e.value` දැනට තෝරාගත් අගය දරයි