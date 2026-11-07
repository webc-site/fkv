# ჩამოსაშლელი შერჩევის ველისა და ღილაკის კომბინაცია

- შეაერთეთ შერჩევის ველი და ღილაკი ჰორიზონტალურად
- ავტომატურად შეინახეთ არჩეული მდგომარეობა localStorage-ში
- ღილაკზე დაწკაპუნების ჩაჭრა და მორგებული მოვლენების გაგზავნა

## გამოიყენეთ დემო ვერსია

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ვარიანტი 1</option>
    <option value="option2">ვარიანტი 2</option>
  </c-select>
  <button>გაშვება</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ქონება

- `key`: გასაღების სახელი გამოიყენება localStorage მდგრადობისთვის
- `value`: ამჟამად არჩეული მნიშვნელობა

## ღონისძიება

- `submit`: იგზავნება ღილაკზე დაწკაპუნებისას, `e.value` შეიცავს ამჟამად არჩეულ მნიშვნელობას