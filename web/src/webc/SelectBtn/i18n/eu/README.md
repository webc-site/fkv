# Goitibeherako hautapen-koadroaren eta botoiaren konbinazioa

- Lotu hautapen-koadroa eta botoia horizontalean
- Gorde automatikoki hautatutako egoera localStorage-n
- Atzeman botoien klikak eta bidali gertaera pertsonalizatuak

## Erabili demoa

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">1. aukera</option>
    <option value="option2">2. aukera</option>
  </c-select>
  <button>korrika</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## jabetza

- `key`: localStorage iraunkorrerako erabiltzen den gako-izena
- `value`: une honetan hautatutako balioa

## gertaera

- `submit`: botoia sakatzean bidaltzen da, `e.value`k unean hautatutako balioa darama